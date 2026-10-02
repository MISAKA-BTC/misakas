//! **RFC-0001 §2.8 and §2.11 at the gateway entrance: decoded tensors and embedding claims.**
//!
//! * **Images are decoded integer tensors only** (RFC-0001 §7 decision 6, adopted 2026-10-03). The chain never decodes a
//!   PNG or a JPEG: a codec is a floating-point, library-versioned program, and a consensus rule that depended on one
//!   would be a rule nobody could replay. So the request carries the pixels the model reads — `u8` HWC RGB, hex, with
//!   its `h` and `w` — as a `palw_image_tensor` content part; a URL, an encoded image or any other kind of part is refused
//!   BY NAME with what to send instead. An admitted image becomes a V5 job's slot reference
//!   ([`kaspa_consensus_core::palw_gen_close_v1::palw_gen_image_input_ref_v1`]): its `input_root` at the slot's tile length,
//!   the bytes travelling with the capture, never on chain.
//! * **An embedding claim is a generative job on an `Embedding` class** (RFC-0003 §II.3), not the local `/v1/embeddings`
//!   service's forward pass: [`embedding_claim_job_v1`] builds the `PalwGenJobV1` (its text commitment in the network's
//!   prompt-id form, the class's pooling and output width) a tensor claim carries.
//!
//! Neither is executed here: this module is the entrance's vocabulary. The image encoder and the tensor worker are the
//! executor's (`misaka-palw-base0`'s `gen_tensor_worker`), and a gateway whose worker serves no such class refuses the
//! request by name at the run, not by dropping the image.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_gen_class_v1::{PalwGenImageInputRefV1, PALW_GEN_POOLING_CLS_V1, PALW_GEN_POOLING_LAST_V1, PALW_GEN_POOLING_MEAN_V1};
use kaspa_consensus_core::palw_gen_job_v1::{
    PALW_GEN_JOB_VERSION_V1, PalwGenBodyV1, PalwGenEmbeddingBodyV1, PalwGenEmbeddingInputV1, PalwGenJobV1, PalwJobEnvelopeV1,
};
use serde_json::{Map, Value};

/// The content-part type that carries a decoded image.
pub const IMAGE_TENSOR_PART: &str = "palw_image_tensor";
/// Most images a request may carry (a V5 job's slot count: 0..=16).
pub const MAX_IMAGES: usize = 16;
/// Largest side, in pixels, the entrance admits (a class's own resolution is the finer check, at admission).
pub const MAX_IMAGE_SIDE: u32 = 4096;
/// Most pixel bytes one image may carry.
pub const MAX_IMAGE_BYTES: usize = 16 << 20;

/// One decoded image: `u8` HWC RGB, `rgb.len() = h · w · 3`, row-major.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodedImageV1 {
    pub h: u32,
    pub w: u32,
    pub rgb: Vec<u8>,
}

/// **A `palw_image_tensor` part** `{"type": "palw_image_tensor", "h": H, "w": W, "format": "u8_hwc_rgb", "data": "<hex>"}`.
/// Every refusal names the field and what was expected.
pub fn parse_image_tensor_part(at: &str, members: &Map<String, Value>) -> Result<DecodedImageV1, String> {
    for key in members.keys() {
        if !matches!(key.as_str(), "type" | "h" | "w" | "format" | "data") {
            return Err(format!("{at}: `{key}` is not a field of a {IMAGE_TENSOR_PART} part (type, h, w, format, data)"));
        }
    }
    let side = |name: &str| -> Result<u32, String> {
        let v = members
            .get(name)
            .and_then(Value::as_u64)
            .ok_or_else(|| format!("{at}: `{name}` is missing or is not a whole number of pixels"))?;
        if v == 0 || v > u64::from(MAX_IMAGE_SIDE) {
            return Err(format!("{at}: `{name}` {v} is outside 1..={MAX_IMAGE_SIDE}"));
        }
        Ok(v as u32)
    };
    let (h, w) = (side("h")?, side("w")?);
    match members.get("format").and_then(Value::as_str) {
        Some("u8_hwc_rgb") => {}
        Some(other) => {
            return Err(format!("{at}: format {other:?} is refused by name: the only form is \"u8_hwc_rgb\" (decoded 8-bit RGB, rows top to bottom)"));
        }
        None => return Err(format!("{at}: `format` is missing: say \"u8_hwc_rgb\"")),
    }
    let data = members.get("data").and_then(Value::as_str).ok_or_else(|| format!("{at}: `data` is missing or is not a hex string"))?;
    let expected = (h as usize) * (w as usize) * 3;
    if expected > MAX_IMAGE_BYTES {
        return Err(format!("{at}: {h}x{w} is {expected} pixel bytes and the cap is {MAX_IMAGE_BYTES}"));
    }
    if data.len() != expected * 2 {
        return Err(format!("{at}: `data` is {} hex characters and {h}x{w}x3 is {} bytes ({} characters)", data.len(), expected, expected * 2));
    }
    let mut rgb = vec![0u8; expected];
    faster_hex::hex_decode(data.as_bytes(), &mut rgb).map_err(|e| format!("{at}: `data` is not hex: {e}"))?;
    Ok(DecodedImageV1 { h, w, rgb })
}

/// **The refusal of every other kind of image part**, by name — what a client that sent a URL, a data URI or an
/// encoded image is told to send instead.
pub fn refuse_encoded_image(at: &str, kind: &str) -> String {
    format!(
        "{at} is a `{kind}` part: images are decoded integer tensors only (RFC-0001 §2.11) — a URL or an encoded image (PNG, JPEG, \
         WebP) would put a codec in the consensus path. Send a `{IMAGE_TENSOR_PART}` part: the decoded `u8` HWC RGB pixels in hex with \
         `h`, `w` and `format` \"u8_hwc_rgb\""
    )
}

/// Whether a part type is one of the image kinds OpenAI-shaped clients send (refused by name above).
pub fn is_encoded_image_kind(kind: &str) -> bool {
    matches!(kind, "image_url" | "input_image" | "image" | "image_file")
}

/// **The V5 slot references of a request's images**, at the class's input tile length: one reference per image, in the
/// order the request carried them (slot order).
pub fn image_slot_refs_v1(images: &[DecodedImageV1], tile_len: u32) -> Result<Vec<PalwGenImageInputRefV1>, String> {
    if images.len() > MAX_IMAGES {
        return Err(format!("{} images exceed the {MAX_IMAGES} a V5 job's slots can carry", images.len()));
    }
    images
        .iter()
        .map(|image| {
            kaspa_consensus_core::palw_gen_close_v1::palw_gen_image_input_ref_v1(
                &misaka_palw_tir::pipeline::JobImageV1 { h: image.h, w: image.w, rgb: image.rgb.clone() },
                tile_len,
            )
        })
        .collect()
}

/// What the class says of an embedding job and the request's own facts, for [`embedding_claim_job_v1`].
pub struct EmbeddingClaimFactsV1 {
    pub envelope: PalwJobEnvelopeV1,
    /// The class's pooling (`PALW_GEN_POOLING_*`).
    pub pooling: u8,
    /// The class's output width.
    pub dims: u32,
}

/// **An embedding claim's job** (RFC-0003 §II.3): the text's ids committed in the network's prompt-id form, under the
/// class's pooling and width. `token_ids` are the class tokenizer's ids of the input (the worker's tokenizer; the entrance
/// does not tokenize).
pub fn embedding_claim_job_v1(
    facts: EmbeddingClaimFactsV1,
    token_ids: &[u32],
    form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
) -> Result<PalwGenJobV1, String> {
    if token_ids.is_empty() {
        return Err("an embedding claim of no tokens has no vector".to_string());
    }
    if !matches!(facts.pooling, PALW_GEN_POOLING_CLS_V1 | PALW_GEN_POOLING_MEAN_V1 | PALW_GEN_POOLING_LAST_V1) {
        return Err(format!("pooling {} is none of the class pooling codes", facts.pooling));
    }
    let token_ids_hash = kaspa_consensus_core::palw_prompt_ids_v1::prompt_token_ids_commitment_v1(form, token_ids)
        .map_err(|e| format!("the ids do not commit in this network's form: {e:?}"))?;
    Ok(PalwGenJobV1 {
        version: PALW_GEN_JOB_VERSION_V1,
        envelope: facts.envelope,
        seed: [0u8; 32],
        body: PalwGenBodyV1::Embedding(PalwGenEmbeddingBodyV1 {
            input: PalwGenEmbeddingInputV1::Text { token_ids_hash, tokens: token_ids.len() as u32 },
            pooling: facts.pooling,
            dims: facts.dims,
            output: misaka_palw_gen::output::OutputKindV1::EmbeddingI32.tag(),
        }),
    })
}

/// The class pooling code of the entrance's pool name (`mean` / `last` / `cls`), for the claim form.
pub fn pooling_code_v1(name: &str) -> Option<u8> {
    match name {
        "cls" => Some(PALW_GEN_POOLING_CLS_V1),
        "mean" => Some(PALW_GEN_POOLING_MEAN_V1),
        "last" => Some(PALW_GEN_POOLING_LAST_V1),
        _ => None,
    }
}

#[allow(dead_code)]
fn _hash_is_used(_: Hash64) {}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn part(h: u64, w: u64, data: String) -> Map<String, Value> {
        json!({"type": IMAGE_TENSOR_PART, "h": h, "w": w, "format": "u8_hwc_rgb", "data": data}).as_object().unwrap().clone()
    }

    #[test]
    fn a_decoded_tensor_is_admitted_and_everything_else_is_refused_by_name() {
        let rgb: Vec<u8> = (0..2 * 3 * 3).map(|i| (i * 7) as u8).collect();
        let ok = parse_image_tensor_part("p", &part(2, 3, faster_hex::hex_string(&rgb))).expect("2x3 RGB");
        assert_eq!((ok.h, ok.w, ok.rgb.as_slice()), (2, 3, rgb.as_slice()));
        let bad = |m: Map<String, Value>| parse_image_tensor_part("p", &m).unwrap_err();
        assert!(bad(part(2, 3, "00".into())).contains("hex characters"), "a short body");
        assert!(bad(part(0, 3, String::new())).contains("outside"), "an empty side");
        assert!(bad(part(5000, 3, String::new())).contains("outside"), "a side past the cap");
        let mut format = part(2, 3, faster_hex::hex_string(&rgb));
        format.insert("format".into(), json!("png"));
        assert!(bad(format).contains("refused by name"));
        let mut extra = part(2, 3, faster_hex::hex_string(&rgb));
        extra.insert("url".into(), json!("http://x"));
        assert!(bad(extra).contains("not a field"));
        assert!(bad(part(2, 3, "zz".repeat(18))).contains("not hex"));
        assert!(refuse_encoded_image("m[0]", "image_url").contains("decoded integer tensors only"));
        assert!(is_encoded_image_kind("image_url") && !is_encoded_image_kind("text"));
    }

    #[test]
    fn an_image_reference_is_the_input_root_the_court_opens_tiles_against() {
        let image = DecodedImageV1 { h: 4, w: 4, rgb: (0..48).collect() };
        let refs = image_slot_refs_v1(&[image.clone(), image.clone()], 16).expect("two slots");
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0], refs[1], "the same pixels, the same root");
        assert_eq!((refs[0].h, refs[0].w), (4, 4));
        let other = DecodedImageV1 { rgb: (1..49).collect(), ..image };
        assert_ne!(image_slot_refs_v1(&[other], 16).unwrap()[0].input_root, refs[0].input_root, "other pixels, another root");
        assert!(image_slot_refs_v1(&vec![DecodedImageV1 { h: 1, w: 1, rgb: vec![0; 3] }; MAX_IMAGES + 1], 16).is_err());
    }

    #[test]
    fn an_embedding_claim_job_commits_its_ids_pooling_and_width() {
        let envelope = PalwJobEnvelopeV1 {
            network_domain: Hash64::from_bytes([1; 64]),
            class_id: Hash64::from_bytes([2; 64]),
            executor_bond: kaspa_consensus_core::tx::TransactionOutpoint::new(kaspa_consensus_core::tx::TransactionId::from_bytes([3; 64]), 0),
            executor_pubkey: vec![4; 32],
            operator_id: Hash64::from_bytes([5; 64]),
            anchor_block: Hash64::from_bytes([6; 64]),
            anchor_daa: 100,
            job_nonce: [7; 32],
            privacy_mode: kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_PRIVACY_PUBLIC_DA,
            prompt_mode: kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_PROMPT_MODE_USER,
        };
        let facts = |pooling| EmbeddingClaimFactsV1 { envelope: envelope.clone(), pooling, dims: 384 };
        let form = kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1;
        let job = embedding_claim_job_v1(facts(pooling_code_v1("mean").unwrap()), &[5, 6, 7], form).expect("a job");
        assert_eq!(PalwGenJobV1::decode_canonical(&job.encode()).unwrap(), job, "its canonical encoding round-trips");
        let PalwGenBodyV1::Embedding(body) = &job.body else { panic!("embedding") };
        assert_eq!((body.pooling, body.dims), (PALW_GEN_POOLING_MEAN_V1, 384));
        assert_eq!(job.text_commitments().1, 3);
        let other = embedding_claim_job_v1(facts(PALW_GEN_POOLING_LAST_V1), &[5, 6, 7], form).unwrap();
        assert_ne!(job.id(), other.id(), "the pooling is in the job id");
        assert!(embedding_claim_job_v1(facts(2), &[], form).is_err(), "no tokens");
        assert!(embedding_claim_job_v1(facts(9), &[1], form).is_err(), "an unknown pooling");
        assert!(pooling_code_v1("max").is_none());
    }
}
