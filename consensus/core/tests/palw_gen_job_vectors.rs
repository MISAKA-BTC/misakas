//! **RFC-0003 §I.0: the generative job's golden vectors** (`consensus-vectors/gen-v1/job_v1.json`,
//! written by `scripts/palw-gen-job-vectors.py`, an implementation that shares no code with this one).
//!
//! Every job decodes from its canonical bytes, re-encodes byte-identical and has the job id the vector
//! says; every non-canonical encoding is refused by name and rewritten by nobody; the canonical seed is
//! the first half of the keyed digest of the anchor.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_gen_job_v1::*;
use kaspa_consensus_core::palw_gen_v1::PalwGenProfileV1;

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex")).collect()
}

fn vectors() -> serde_json::Value {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../consensus-vectors/gen-v1/job_v1.json");
    serde_json::from_slice(&std::fs::read(path).expect("the job vectors")).expect("json")
}

#[test]
fn every_job_decodes_from_its_canonical_bytes_and_has_the_vectors_id() {
    let v = vectors();
    assert_eq!(v["job_id_key"].as_str().unwrap().as_bytes(), PALW_GEN_JOB_ID_DOMAIN_V1);
    let cases = v["cases"].as_array().unwrap();
    assert!(cases.len() >= 4);
    for c in cases {
        let name = c["name"].as_str().unwrap();
        let bytes = unhex(c["borsh_hex"].as_str().unwrap());
        let job = PalwGenJobV1::decode_canonical(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(job.encode(), bytes, "{name}: re-encodes byte-identical");
        assert_eq!(job.id(), Hash64::from_bytes(unhex(c["job_id_hex"].as_str().unwrap()).try_into().unwrap()), "{name}: the job id");
        assert_eq!(job.seed.to_vec(), unhex(c["seed_hex"].as_str().unwrap()), "{name}: the seed");
        let profile = match c["profile"].as_str().unwrap() {
            "image" => PalwGenProfileV1::Image,
            "embedding" => PalwGenProfileV1::Embedding,
            p => panic!("{name}: profile {p}"),
        };
        assert_eq!(job.body.profile(), profile, "{name}");
        assert_eq!(job.version, PALW_GEN_JOB_VERSION_V1);
    }
}

#[test]
fn every_non_canonical_encoding_is_refused_by_name() {
    for c in vectors()["not_canonical"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let bytes = unhex(c["borsh_hex"].as_str().unwrap());
        assert!(matches!(PalwGenJobV1::decode_canonical(&bytes), Err(PalwGenJobErrorV1::NotCanonical(_))), "{name}");
    }
}

#[test]
fn the_canonical_seed_is_the_first_half_of_the_anchors_keyed_digest() {
    let v = vectors();
    assert_eq!(v["canonical_seed_key"].as_str().unwrap().as_bytes(), PALW_GEN_CANONICAL_SEED_DOMAIN_V1);
    for s in v["canonical_seeds"].as_array().unwrap() {
        let anchor = Hash64::from_bytes(unhex(s["anchor_hex"].as_str().unwrap()).try_into().unwrap());
        assert_eq!(palw_gen_canonical_seed_v1(&anchor).to_vec(), unhex(s["seed_hex"].as_str().unwrap()));
    }
}
