//! **FP Job V5 compiled in, FP Job V4 unchanged** (RFC-0003 §II.2.1; RFC-0001 §A).
//!
//! * **V4's wire and ids**: every case of `consensus-vectors/fp-v4/job_v4_encoding.json` decodes,
//!   re-encodes to the same bytes and hashes to the same job id through the lane's own functions, and
//!   V4's version rules admit it exactly as before.
//! * **V4's decoder** — what the worker drives, the seat replays and the court's decode-token door
//!   re-derives — passes every other `fp-v4` golden vector through the lane's public checkers.
//! * **Isolation**: a V5 job's bytes are a V4 job's with the version word 8 and the images after; no
//!   V3/V4 rule admits version 8, no V4 byte string decodes as V5, and V5's id is under its own key.
//! * **V5's own rules**: the shape, OQ14 (a class with image slots takes V5 only), the images one per
//!   slot, the context, and OQ13's price (pending user confirmation).
//! * **The fence**: dormant on every shipped ruleset, in no testnet-12 list, Some-only in the params
//!   and schedule ids, collapsed from `never()` for the identity, named by the fork id, refused
//!   without its prerequisites.
//!
//! The lane's claims and court run their own suites unchanged beside this one
//! (`misaka-palw-base0`'s `fp_job_v4*`, `palw_court_decode_close_door`), and the shipped rulesets'
//! ids are pinned in `palw_tir_fences_are_dormant.rs` and `palw_the_release_did_not_move.rs`.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::{
    ForkActivation, MAINNET_PARAMS, PALW_T12_POST_LAUNCH_FENCES_V1, PALW_T12_POST_LAUNCH_FENCES_V2, PALW_T12_POST_LAUNCH_FENCES_V3,
    PALW_T12_TIR_FLAG_DAY_FENCES_V1, Params, SIMNET_PARAMS, TESTNET11_PARAMS, mainnet_shipped_params, palw_t12_release_v3_params,
    palw_t12_shipped_params,
};
use kaspa_consensus_core::fork_id_v1::fork_id_gate_fences_v1;
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_decode_pipeline_v4::{DecodeConfigV4, PalwFpDecodeStopV1, PalwFpDecoderV1, decode_select_v4};
use kaspa_consensus_core::palw_decode_select_v2::PalwDecodeSamplingV2;
use kaspa_consensus_core::palw_fp_job_v5::*;
use kaspa_consensus_core::palw_fp_v4_vectors::*;
use kaspa_consensus_core::palw_freeprompt_v3::{
    PALW_FP_V3_ALL_DOMAINS, PALW_FP_V3_VERSION, PALW_FP_V4_VERSION, PalwFpDecodeRulesV1, PalwFpV3Error, PalwFreePromptJobV3,
    fp_job_id_v3, fp_job_id_v4,
};
use kaspa_consensus_core::palw_gen_class_v1::{PalwGenImageInputRefV1, PalwGenImageOfferV1, PalwGenJobImageErrorV1, PalwGenOffersV1};
use kaspa_consensus_core::palw_gen_v1::PalwGenFenceV1;

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex")).collect()
}

fn vector(name: &str) -> String {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../consensus-vectors/fp-v4").join(name);
    std::fs::read_to_string(path).expect("an fp-v4 vector")
}

/// Every job of `job_v4_encoding.json`: `(name, bytes, job id hex)`.
fn v4_cases() -> Vec<(String, Vec<u8>, String)> {
    let v: serde_json::Value = serde_json::from_str(&vector("job_v4_encoding.json")).expect("json");
    v["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            (
                c["name"].as_str().unwrap().to_string(),
                unhex(c["borsh_hex"].as_str().unwrap()),
                c["job_id"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

fn v4_jobs() -> Vec<PalwFreePromptJobV3> {
    v4_cases()
        .iter()
        .map(|(_, b, _)| borsh::from_slice(b).unwrap())
        .filter(|j: &PalwFreePromptJobV3| j.version == PALW_FP_V4_VERSION)
        .collect()
}

fn image(h: u32, w: u32) -> PalwGenImageInputRefV1 {
    PalwGenImageInputRefV1 { input_root: Hash64::from_bytes([0x3b; 64]), h, w }
}

fn offers(slots: &[(u32, u32, u32)]) -> PalwGenOffersV1 {
    PalwGenOffersV1 {
        steps: vec![],
        scalars: vec![],
        max_prompt_tokens: 64,
        max_negative_tokens: 0,
        images: slots.iter().map(|(h, w, t)| PalwGenImageOfferV1 { h: *h, w: *w, tile_len: 4, token_equivalents: *t }).collect(),
    }
}

#[test]
fn v4_wire_ids_and_version_rules_are_the_vectors() {
    let cases = v4_cases();
    assert_eq!(cases.len(), 5);
    for (name, bytes, id) in &cases {
        let job: PalwFreePromptJobV3 = borsh::from_slice(bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(&borsh::to_vec(&job).unwrap(), bytes, "{name}: the bytes");
        assert_eq!(fp_job_id_v3(&job).to_string(), *id, "{name}: the job id");
        match job.version {
            PALW_FP_V4_VERSION => {
                assert_eq!(fp_job_id_v4(&job), fp_job_id_v3(&job), "{name}: the V4 id is the lane's one id function");
                assert_eq!(PalwFpDecodeRulesV1::Active.check_job(&job), Ok(()), "{name}: V4 past the fence");
                assert_eq!(PalwFpDecodeRulesV1::Dormant.check_job(&job), Err(PalwFpV3Error::DecodeRulesNotArmed), "{name}");
            }
            PALW_FP_V3_VERSION => {
                assert_eq!(PalwFpDecodeRulesV1::Dormant.check_job(&job), Ok(()), "{name}: V3 below the fence");
                assert_eq!(PalwFpDecodeRulesV1::Active.check_job(&job), Err(PalwFpV3Error::V3JobPastDecodeRules), "{name}");
            }
            other => panic!("{name}: version {other}"),
        }
    }
}

#[test]
fn v4s_decoder_passes_every_golden_vector() {
    let run = |config: &DecodeConfigV4, limit: u32, rows: &[Vec<i32>]| -> (Vec<u32>, Vec<u32>, Option<PalwFpDecodeStopV1>) {
        let mut decoder = PalwFpDecoderV1::v4(config.clone(), PalwDecodeSamplingV2::GREEDY, limit);
        let fed = rows.iter().map(|row| decoder.select(row)).collect();
        (fed, decoder.generated().to_vec(), decoder.stop())
    };
    let results = [
        fp_v4_check_repeat_vectors(&vector("repeat_penalty.json")),
        fp_v4_check_penalty_vectors(&vector("frequency_penalty.json")),
        fp_v4_check_penalty_vectors(&vector("presence_penalty.json")),
        fp_v4_check_logit_bias_vectors(&vector("logit_bias.json")),
        fp_v4_check_stop_vectors(&vector("stop_sequences.json")),
        fp_v4_check_processor_vectors(&vector("processor_order.json"), &decode_select_v4, &run),
        fp_v4_check_noop_vectors(
            &vector("v4_noop_equals_v3.json"),
            &|s, t, row| kaspa_consensus_core::palw_decode_select_v2::decode_token_select_v2(row, &s.seed, t, s.temperature_q),
            &decode_select_v4,
        ),
    ];
    let mut total = 0;
    for (file, result) in FP_V4_VECTOR_FILES.iter().zip(results) {
        let n = result.unwrap_or_else(|e| panic!("{file}: {e}"));
        assert!(n > 0, "{file}");
        total += n;
    }
    assert!(total > 500, "{total}");
}

#[test]
fn a_v5_job_is_a_v4_job_with_version_8_and_its_images() {
    for v4 in v4_jobs() {
        let v5 = PalwFreePromptJobV5 { v4: v4.clone(), images: vec![image(2, 3), image(4, 4)] };
        let bytes = borsh::to_vec(&v5).unwrap();
        let v4_bytes = borsh::to_vec(&v4).unwrap();
        assert_eq!(bytes[..2], PALW_FP_V5_VERSION.to_le_bytes());
        assert_eq!(bytes[2..v4_bytes.len()], v4_bytes[2..], "every V4 field and its decode rules, unchanged");
        assert_eq!(bytes[v4_bytes.len()..], borsh::to_vec(&v5.images).unwrap()[..], "then the images");
        let back: PalwFreePromptJobV5 = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, v5, "round trip, the embedded job a V4 job again");
        assert_eq!(back.v4.version, PALW_FP_V4_VERSION);
        assert_eq!(v5.validate_shape_v1(), Ok(()));
        // No V3/V4 rule admits version 8, and V4 bytes are not a V5 job.
        let as_v4: PalwFreePromptJobV3 = borsh::BorshDeserialize::deserialize(&mut &bytes[..]).unwrap();
        assert_eq!(as_v4.version, PALW_FP_V5_VERSION);
        for rule in [PalwFpDecodeRulesV1::Dormant, PalwFpDecodeRulesV1::Scheduled, PalwFpDecodeRulesV1::Active] {
            assert!(matches!(rule.check_job(&as_v4), Err(PalwFpV3Error::UnsupportedVersion { got: 8, .. })), "{rule:?}");
        }
        // The lane's job type reads the whole of it as the V5 job it carries (version 8, its images
        // the tail), which every V3/V4 rule refuses by version.
        assert_eq!(borsh::from_slice::<PalwFreePromptJobV3>(&bytes).unwrap(), v5.into_carried(), "the carried form");
        assert!(borsh::from_slice::<PalwFreePromptJobV5>(&v4_bytes).is_err(), "version 7 is not V5");
        // Its own id, under its own key.
        assert_ne!(fp_job_id_v5(&v5), fp_job_id_v4(&v4));
        let mut other = v5.clone();
        other.images[1].input_root = Hash64::from_bytes([0x3c; 64]);
        assert_ne!(fp_job_id_v5(&v5), fp_job_id_v5(&other), "the images are in the id");
    }
    assert!(PALW_FP_V3_ALL_DOMAINS.iter().all(|d| *d != PALW_FP_V5_DOMAIN_JOB_ID), "a domain no V3/V4 id uses");
}

#[test]
fn v5s_own_rules_refuse_by_name() {
    let v4 = v4_jobs().remove(0);
    let job = |images: Vec<PalwGenImageInputRefV1>| PalwFreePromptJobV5 { v4: v4.clone(), images };
    // The shape.
    let mut v3_inside = job(vec![image(2, 3)]);
    v3_inside.v4.version = PALW_FP_V3_VERSION;
    v3_inside.v4.decode = None;
    assert!(matches!(v3_inside.validate_shape_v1(), Err(PalwFpV5Error::NotAV4Job { version: 5, decode_present: false })));
    let mut bad_v4 = job(vec![image(2, 3)]);
    bad_v4.v4.decode = Some(DecodeConfigV4 { penalty_window: 3, ..DecodeConfigV4::NOOP });
    assert!(matches!(bad_v4.validate_shape_v1(), Err(PalwFpV5Error::V4(_))), "V4's own canonical form");
    assert_eq!(job(vec![]).validate_shape_v1(), Err(PalwFpV5Error::ImageCount(0)), "a text-only job is V4");
    assert_eq!(job(vec![image(1, 1); 17]).validate_shape_v1(), Err(PalwFpV5Error::ImageCount(17)));
    // OQ14: a class with image slots takes V5 only; one without, V4 only.
    let vlm = offers(&[(2, 3, 5)]);
    let text = offers(&[]);
    assert_eq!(palw_fp_job_version_offered_v1(&vlm, true), Ok(()));
    assert_eq!(palw_fp_job_version_offered_v1(&vlm, false), Err(PalwFpV5Error::JobVersionNotOffered { job: "FP Job V4" }));
    assert_eq!(palw_fp_job_version_offered_v1(&text, false), Ok(()));
    assert_eq!(palw_fp_job_version_offered_v1(&text, true), Err(PalwFpV5Error::JobVersionNotOffered { job: "FP Job V5" }));
    // Admission: the fence, the images one per slot at its size, the context.
    let ok = job(vec![image(2, 3)]);
    let context = v4.prompt_tokens + v4.decode_token_limit - 1;
    assert_eq!(palw_fp_job_v5_admitted_v1(&ok, &vlm, context, false), Err(PalwFpV5Error::NotArmed));
    assert_eq!(palw_fp_job_v5_admitted_v1(&ok, &vlm, context, true), Ok(()));
    assert!(matches!(palw_fp_job_v5_admitted_v1(&ok, &vlm, context - 1, true), Err(PalwFpV5Error::ContextExceeded { .. })));
    assert_eq!(
        palw_fp_job_v5_admitted_v1(&job(vec![image(3, 2)]), &vlm, context, true),
        Err(PalwFpV5Error::Images(PalwGenJobImageErrorV1::ImageSizeNotOffered { index: 0, h: 2, w: 3, got_h: 3, got_w: 2 }))
    );
    assert!(matches!(
        palw_fp_job_v5_admitted_v1(&job(vec![image(2, 3), image(2, 3)]), &vlm, context, true),
        Err(PalwFpV5Error::Images(PalwGenJobImageErrorV1::ImageCount { want: 1, got: 2 }))
    ));
    assert!(matches!(palw_fp_job_v5_admitted_v1(&ok, &text, context, true), Err(PalwFpV5Error::JobVersionNotOffered { .. })));
    // OQ13 (pending user confirmation): each slot's prompt tokens, at the job's per-token price.
    let two = offers(&[(2, 3, 5), (4, 4, 7)]);
    assert_eq!(palw_fp_v5_image_tokens_v1(&two), 12);
    assert_eq!(palw_fp_v5_image_charge_v1(&two, 1_000), 12_000);
    assert_eq!(palw_fp_v5_image_tokens_v1(&text), 0);
}

fn ids(p: &Params) -> (String, String, String) {
    (p.consensus_params_id().to_string(), p.consensus_identity_id().to_string(), p.consensus_schedule_id().to_string())
}

/// testnet-12 as shipped with the generative fence and FP Job V4's decode rules armed below `at`:
/// the prerequisites V5 names.
fn t12_with_prerequisites(at: u64) -> Params {
    let mut p = palw_t12_shipped_params();
    p.palw_gen_v1 = Some(PalwGenFenceV1::drill_v1(ForkActivation::new(at - 10)));
    p.palw_fp_decode_rules = Some(ForkActivation::new(at - 20));
    p.sync_palw_fp_decode_rules();
    p
}

#[test]
fn the_v5_fence_is_dormant_everywhere_and_fingerprinted_only_when_armed() {
    for (name, p) in [
        ("MAINNET_PARAMS", MAINNET_PARAMS),
        ("TESTNET11_PARAMS", TESTNET11_PARAMS),
        ("SIMNET_PARAMS", SIMNET_PARAMS),
        ("mainnet_shipped_params", mainnet_shipped_params()),
        ("palw_t12_shipped_params", palw_t12_shipped_params()),
        ("palw_t12_release_v3_params", palw_t12_release_v3_params()),
        ("from(testnet-12)", Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12))),
    ] {
        assert!(p.palw_fp_job_v5.is_none() && p.palw_fp_job_v5_fence().is_none(), "{name}: dormant");
        assert!(p.palw_fences_v1().contains(&("palw_fp_job_v5", None)), "{name}: named");
        assert!(p.validate_palw_fp_job_v5_v1().is_ok(), "{name}");
    }
    for list in [
        PALW_T12_POST_LAUNCH_FENCES_V1,
        PALW_T12_POST_LAUNCH_FENCES_V2,
        PALW_T12_POST_LAUNCH_FENCES_V3,
        PALW_T12_TIR_FLAG_DAY_FENCES_V1,
    ] {
        assert!(list.iter().all(|f| f.name != "palw_fp_job_v5"), "no testnet-12 release arms it");
    }
    const AT: u64 = 9_999_995;
    let base = t12_with_prerequisites(AT);
    base.validate_palw_v2().unwrap_or_else(|e| panic!("the prerequisites validate: {e}"));
    let mut armed = base.clone();
    (PALW_DRILL_FP_V5_ENTRY.set)(&mut armed, Some(ForkActivation::new(AT)));
    armed.validate_palw_v2().unwrap_or_else(|e| panic!("V5 over its prerequisites: {e}"));
    let (b, a) = (ids(&base), ids(&armed));
    assert_ne!(a.0, b.0, "armed, the params id names it");
    assert_eq!(a.1, b.1, "a future height does not split the identity");
    assert_ne!(a.2, b.2, "and the schedule reports it");
    assert!(armed.palw_fp_job_v5_active_at(AT) && !armed.palw_fp_job_v5_active_at(AT - 1));
    assert!(fork_id_gate_fences_v1(&armed).contains(&AT));
    let mut never = base.clone();
    never.palw_fp_job_v5 = Some(ForkActivation::never());
    assert_eq!(never.consensus_identity_id(), base.consensus_identity_id(), "never() collapses for the identity");
    (PALW_DRILL_FP_V5_ENTRY.set)(&mut armed, None);
    assert_eq!(ids(&armed), b, "setting it back is the ruleset, byte for byte");
    // Its prerequisites, each by name.
    let with = |edit: &dyn Fn(&mut Params)| {
        let mut p = base.clone();
        p.palw_fp_job_v5 = Some(ForkActivation::new(AT));
        edit(&mut p);
        p.validate_palw_fp_job_v5_v1()
    };
    assert!(with(&|_| ()).is_ok());
    assert!(with(&|p| p.palw_gen_v1 = None).is_err(), "no generative fence");
    assert!(with(&|p| p.palw_gen_v1 = Some(PalwGenFenceV1::drill_v1(ForkActivation::new(AT + 1)))).is_err(), "a later one");
    assert!(with(&|p| p.palw_fp_decode_rules = None).is_err(), "no FP Job V4");
    assert!(with(&|p| p.palw_fp_decode_rules = Some(ForkActivation::new(AT + 1))).is_err(), "a later one");
    let mut not_v2 = SIMNET_PARAMS;
    not_v2.palw_fp_job_v5 = Some(ForkActivation::new(AT));
    assert!(not_v2.validate_palw_fp_job_v5_v1().is_err(), "a ConsensusV2 rule");
    assert!(not_v2.palw_fp_job_v5_fence().is_none());
}

/// A V4 commitment payload in the lane's shape (the walk's own test fixture's fields).
fn payload_v4(v4: &PalwFreePromptJobV3) -> kaspa_consensus_core::palw_freeprompt_v3::PalwFpCommitmentTxPayloadV3 {
    use kaspa_consensus_core::palw_freeprompt_v3::{PalwFpCommitmentTxPayloadV3, PalwFpStopReasonV3, PalwFreePromptCommitmentV3};
    let h = |w: u64| Hash64::from_u64_word(w);
    let commitment = PalwFreePromptCommitmentV3 {
        job: v4.clone(),
        trace_root: h(0x7A),
        output_root: h(0x0B),
        schedule_root: h(0x5C),
        execution_root: h(0x4E),
        decode_tokens_executed: 4,
        stop_reason: PalwFpStopReasonV3::EndOfGeneration,
        work_leaves: 64,
        trace_manifest_root: h(0x3F),
        trace_chunk_count: 1,
        trace_retention_daa: 505_000,
    };
    PalwFpCommitmentTxPayloadV3 { version: PALW_FP_V3_VERSION, commitment, prompt_token_ids: vec![], signature: vec![0x5A; 16] }
}

/// **V5 in commitments and claims**: the lane's job type carries a V5 job as version 8 with its
/// images after the V4 tail — the wrapper's bytes exactly — so every wrapper (the commitment, the
/// payload, the claim id, the signed message) carries it unchanged in layout; the V4 validators
/// refuse it by name, and the V5 validator admits it past its fence over V4's own commitment rules.
#[test]
fn a_v5_job_rides_the_lanes_commitment_and_names_its_own_claim() {
    use kaspa_consensus_core::palw_freeprompt_v3::PalwFpCommitmentTxPayloadV3;
    let v4 = v4_jobs()
        .into_iter()
        .find(|j| j.privacy_mode == kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_PRIVACY_PANEL_DA)
        .unwrap_or_else(|| v4_jobs().remove(0));
    let v5 = PalwFreePromptJobV5 { v4: v4.clone(), images: vec![image(2, 3)] };
    let carried = v5.into_carried();
    assert_eq!(borsh::to_vec(&carried).unwrap(), borsh::to_vec(&v5).unwrap(), "the carried form is the wire");
    assert_eq!(fp_job_id_v3(&carried), fp_job_id_v5(&v5), "the lane's one id function names V5 under V5's key");
    assert_eq!(PalwFreePromptJobV5::from_carried(&carried), Ok(v5.clone()));
    assert_eq!(PalwFreePromptJobV5::from_carried(&v4), Err(PalwFpV5Error::NotAV5Job { version: PALW_FP_V4_VERSION }));
    assert!(carried.is_v5() && !carried.is_v4());
    // Images on a V3 or V4 job are refused by name, before anything else reads them.
    let mut v4_with_images = v4.clone();
    v4_with_images.images = Some(vec![image(2, 3)]);
    assert_eq!(PalwFpDecodeRulesV1::Active.check_job(&v4_with_images), Err(PalwFpV3Error::ImagesVersionMismatch { version: 7 }));
    // The payload and its claim.
    let p4 = payload_v4(&v4);
    let mut p5 = p4.clone();
    p5.commitment.job = carried.clone();
    let bytes = borsh::to_vec(&p5).unwrap();
    assert_eq!(borsh::from_slice::<PalwFpCommitmentTxPayloadV3>(&bytes).unwrap(), p5, "the payload round-trips with its V5 job");
    assert_ne!(p5.claim_id(), p4.claim_id(), "its own claim");
    let form = kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat;
    let ladder = 1 << 26;
    // The V4 walk's validator refuses it at every decode rule: the live lane carries no V5 claim.
    for rule in [PalwFpDecodeRulesV1::Dormant, PalwFpDecodeRulesV1::Scheduled, PalwFpDecodeRulesV1::Active] {
        assert!(p5.validate_stateless_under_ruleset_v4(v4.network_domain, true, ladder, None, form, rule).is_err(), "{rule:?}");
    }
    // The V5 validator: V4's commitment rules over the V4 view, past V5's fence.
    let v4_verdict = p4.validate_stateless_under_ruleset_v4(v4.network_domain, true, ladder, None, form, PalwFpDecodeRulesV1::Active);
    let v5_verdict = palw_fp_v5_validate_payload_v1(&p5, v4.network_domain, true, ladder, None, form, true);
    match v4_verdict {
        Ok(()) => assert_eq!(v5_verdict, Ok(v5.clone()), "V5 is admitted exactly when its V4 view is"),
        Err(e) => assert_eq!(v5_verdict, Err(PalwFpV5Error::V4(e)), "and refused with its V4 view's reason"),
    }
    assert_eq!(palw_fp_v5_validate_payload_v1(&p5, v4.network_domain, true, ladder, None, form, false), Err(PalwFpV5Error::NotArmed));
    assert_eq!(
        palw_fp_v5_validate_payload_v1(&p4, v4.network_domain, true, ladder, None, form, true),
        Err(PalwFpV5Error::NotAV5Job { version: PALW_FP_V4_VERSION })
    );
    // The signature covers the V5 claim id — the lane's own signed message.
    assert_eq!(p5.signed_message(), p5.claim_id());
}
