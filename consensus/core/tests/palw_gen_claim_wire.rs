//! **RFC-0003 §I.4: the tensor claim's wire, its commitment's rules and its three doors** — FP job
//! version 10, dormant behind `palw_fp_job_v5` over `palw_gen_v1`.
//!
//! * a tensor job rides the lane's job type at version 10 (the envelope's fields, canonical zeros for the
//!   text fields it has not, no decode config, then the tail) and comes back whole; its id is the
//!   generative job's; no V3, V4 or V5 byte string reads as one;
//! * a commitment's fields are §I.4.2's, its execution root is the tensor execution root of its own parts,
//!   its ids (where carried) are the body's — every refusal by name;
//! * the doors: the isolation door admits a version-10 payload only where the ruleset carries the fence (the
//!   lane's own door refuses it by name everywhere else), the header-context door only from the fence's
//!   height, and the acceptance walk turns exactly the admissible payloads into `GenTensorCommitted`.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN;
use kaspa_consensus_core::palw_freeprompt_v3::{
    PALW_FP_PRIVACY_PANEL_DA, PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_PROMPT_MODE_USER, PALW_FP_STRUCTURAL_WORK_LEAVES_CAP, PALW_FP_V3_VERSION,
    PALW_FP_V4_VERSION, PalwFpCommitmentTxPayloadV3, PalwFpDecodeRulesV1, PalwFpJobTailV1, PalwFpStopReasonV3, PalwFpV3Error,
    PalwFreePromptJobV3, fp_claim_id_v3, fp_job_id_v3,
};
use kaspa_consensus_core::palw_decode_pipeline_v4::DecodeConfigV4;
use kaspa_consensus_core::palw_gen_claim_v1::*;
use kaspa_consensus_core::palw_gen_class_v1::PalwGenImageInputRefV1;
use kaspa_consensus_core::palw_gen_job_v1::*;
use kaspa_consensus_core::palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_commitment_v1};
use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;
use kaspa_consensus_core::subnets::{SUBNETWORK_ID_NATIVE, SUBNETWORK_ID_PALW_FP_COMMITMENT};
use kaspa_consensus_core::tx::{Transaction, TransactionId, TransactionOutpoint};

const FORM: PalwPromptIdsFormV1 = PalwPromptIdsFormV1::Flat;
const LEAVES: u64 = 40;

fn h(n: u8) -> Hash64 {
    Hash64::from_bytes([n; 64])
}

fn net() -> Hash64 {
    h(0xD0)
}

fn envelope(privacy: u8) -> PalwJobEnvelopeV1 {
    PalwJobEnvelopeV1 {
        network_domain: net(),
        class_id: h(0xC1),
        executor_bond: TransactionOutpoint::new(TransactionId::from_bytes([7; 64]), 3),
        executor_pubkey: vec![0xAB; 16],
        operator_id: h(0x0E),
        anchor_block: h(0xA1),
        anchor_daa: 4_242,
        job_nonce: [0x9C; 32],
        privacy_mode: privacy,
        prompt_mode: PALW_FP_PROMPT_MODE_USER,
    }
}

/// An image job with a prompt of three ids and (optionally) a negative prompt of two.
fn image_job(privacy: u8, negative: bool) -> (PalwGenJobV1, Vec<u32>) {
    let prompt = vec![3u32, 1, 4];
    let negative_ids = if negative { vec![9u32, 2] } else { vec![] };
    let body = PalwGenImageBodyV1 {
        prompt_token_ids_hash: prompt_token_ids_commitment_v1(FORM, &prompt).unwrap(),
        prompt_tokens: prompt.len() as u32,
        negative_token_ids_hash: if negative { prompt_token_ids_commitment_v1(FORM, &negative_ids).unwrap() } else { Hash64::default() },
        negative_tokens: negative_ids.len() as u32,
        guidance_q: 16,
        image_index: 2,
        sampler_id: h(0x5A),
        steps: 4,
        width: 8,
        height: 8,
        output: 1,
    };
    let job = PalwGenJobV1 { version: PALW_GEN_JOB_VERSION_V1, envelope: envelope(privacy), seed: [0x33; 32], body: PalwGenBodyV1::Image(body) };
    let mut ids = prompt;
    ids.extend(negative_ids);
    (job, ids)
}

/// An embedding of one image.
fn embedding_job() -> PalwGenJobV1 {
    PalwGenJobV1 {
        version: PALW_GEN_JOB_VERSION_V1,
        envelope: envelope(PALW_FP_PRIVACY_PANEL_DA),
        seed: [0; 32],
        body: PalwGenBodyV1::Embedding(PalwGenEmbeddingBodyV1 {
            input: PalwGenEmbeddingInputV1::Image(PalwGenImageInputRefV1 { input_root: h(0x1F), h: 4, w: 4 }),
            pooling: 0,
            dims: 4,
            output: 3,
        }),
    }
}

fn signature() -> Vec<u8> {
    vec![7u8; MLDSA87_SIGNATURE_LEN]
}

fn payload_of(job: &PalwGenJobV1, ids: Vec<u32>) -> PalwFpCommitmentTxPayloadV3 {
    palw_gen_payload_v1(job, LEAVES, h(11), h(12), ids, signature())
}

/// A tensor payload cut short: its job version word is still 10, and it does not decode.
fn torn_payload(payload: &PalwFpCommitmentTxPayloadV3) -> Vec<u8> {
    let mut bytes = borsh::to_vec(payload).unwrap();
    bytes.truncate(40);
    bytes
}

fn tx_of(payload: &PalwFpCommitmentTxPayloadV3) -> Transaction {
    Transaction::new(0, vec![], vec![], 0, SUBNETWORK_ID_PALW_FP_COMMITMENT, 0, borsh::to_vec(payload).unwrap())
}

/// The check at the walk's arguments: this network, `PanelDa` armed, the structural ladder.
fn check(payload: &PalwFpCommitmentTxPayloadV3) -> Result<PalwGenJobV1, PalwGenClaimErrorV1> {
    palw_fp_gen_claim_check_v1(payload, Some(net()), true, PALW_FP_STRUCTURAL_WORK_LEAVES_CAP, FORM)
}

/// The commitment edited, its execution root left as it was.
fn edited(mut payload: PalwFpCommitmentTxPayloadV3, edit: impl FnOnce(&mut PalwFpCommitmentTxPayloadV3)) -> PalwFpCommitmentTxPayloadV3 {
    edit(&mut payload);
    payload
}

// ---------------------------------------------------------------------------------------------
// The wire
// ---------------------------------------------------------------------------------------------

#[test]
fn a_tensor_job_rides_the_lanes_job_type_and_comes_back_whole() {
    for (job, _) in [image_job(PALW_FP_PRIVACY_PUBLIC_DA, true), image_job(PALW_FP_PRIVACY_PANEL_DA, false), (embedding_job(), vec![])] {
        let carried = job.into_carried();
        assert_eq!(carried.version, PALW_FP_GEN_VERSION);
        assert!(carried.decode.is_none(), "no decode config");
        assert!(carried.is_gen_tensor() && !carried.is_v4() && !carried.is_v5());
        assert!(matches!(&carried.tail, Some(PalwFpJobTailV1::Gen(tail)) if tail.seed == job.seed && tail.body == job.body));
        // The lane's envelope fields are the envelope's; every text field the envelope has not is the canonical zero.
        let e = &job.envelope;
        assert_eq!(
            (carried.network_domain, carried.class_id, carried.executor_bond, carried.operator_id, carried.anchor_block, carried.anchor_daa),
            (e.network_domain, e.class_id, e.executor_bond, e.operator_id, e.anchor_block, e.anchor_daa)
        );
        assert_eq!((carried.executor_pubkey.clone(), carried.job_nonce, carried.privacy_mode, carried.prompt_mode), (e.executor_pubkey.clone(), e.job_nonce, e.privacy_mode, e.prompt_mode));
        assert_eq!(
            (carried.tokenizer_id, carried.prompt_token_ids_hash, carried.prompt_tokens, carried.decode_token_limit, carried.max_context_tokens),
            (Hash64::default(), Hash64::default(), 0, 0, 0)
        );
        assert_eq!((carried.sampling_seed, carried.temperature_q), ([0u8; 32], 0));
        // The wire: Borsh round trip, the version word at the V3 position, the tail after the V3 fields.
        let bytes = borsh::to_vec(&carried).unwrap();
        assert_eq!(&bytes[..2], &PALW_FP_GEN_VERSION.to_le_bytes());
        let back: PalwFreePromptJobV3 = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back, carried);
        assert_eq!(PalwGenJobV1::from_carried(&back), Ok(job.clone()));
        // The id is the generative job's, whatever the lane's id function is asked.
        assert_eq!(fp_job_id_v3(&carried), job.id());
        assert_eq!(fp_job_id_gen_carried_v1(&carried), job.id());
        // The tail is exactly what follows the V3 fields: its own Borsh ends the job's.
        assert!(bytes.ends_with(&borsh::to_vec(&job.tail()).unwrap()));
    }
}

#[test]
fn no_other_job_version_reads_as_a_tensor_job_and_none_admits_one() {
    let (job, _) = image_job(PALW_FP_PRIVACY_PUBLIC_DA, false);
    let carried = job.into_carried();
    // The lane's own validators refuse version 10 by name, at every decode rule.
    for rules in [PalwFpDecodeRulesV1::Dormant, PalwFpDecodeRulesV1::Scheduled, PalwFpDecodeRulesV1::Active] {
        assert_eq!(rules.check_job(&carried), Err(PalwFpV3Error::UnsupportedVersion { got: PALW_FP_GEN_VERSION, expected: PALW_FP_V3_VERSION }));
    }
    // A V4 job's bytes with the version word turned to 10 is not a tensor job: its tail is not a tail.
    let mut v4 = carried.clone();
    v4.version = PALW_FP_V4_VERSION;
    v4.tail = None;
    v4.decode = Some(DecodeConfigV4::NOOP);
    let mut bytes = borsh::to_vec(&v4).unwrap();
    bytes[..2].copy_from_slice(&PALW_FP_GEN_VERSION.to_le_bytes());
    let read = borsh::from_slice::<PalwFreePromptJobV3>(&bytes);
    assert!(read.is_err() || PalwGenJobV1::from_carried(read.as_ref().unwrap()).is_err(), "a V4 job renumbered is no tensor job");
    // And a tensor job renumbered to V4 or V5 is no V4 or V5 job (its tail is a tensor tail, whatever the word says).
    let mut as_v4 = borsh::to_vec(&carried).unwrap();
    as_v4[..2].copy_from_slice(&PALW_FP_V4_VERSION.to_le_bytes());
    assert!(borsh::from_slice::<PalwFreePromptJobV3>(&as_v4).is_err() || PalwGenJobV1::from_carried(&borsh::from_slice(&as_v4).unwrap()).is_err());
    // V3, V4 and V5 jobs carry no tensor tail: the tail enum's V5 arm is the only one a V5 job reads.
    assert!(matches!(PalwGenJobV1::from_carried(&PalwFreePromptJobV3 { version: PALW_FP_V3_VERSION, ..carried.clone() }), Err(PalwGenClaimErrorV1::NotATensorJob { .. })));
}

#[test]
fn a_tensor_jobs_text_fields_are_the_canonical_zeros_and_nothing_else() {
    use PalwGenClaimErrorV1::ShellNotCanonical as Shell;
    let (job, _) = image_job(PALW_FP_PRIVACY_PUBLIC_DA, false);
    let good = job.into_carried();
    assert_eq!(PalwGenJobV1::from_carried(&good), Ok(job.clone()));
    let with = |edit: &dyn Fn(&mut PalwFreePromptJobV3)| {
        let mut j = good.clone();
        edit(&mut j);
        PalwGenJobV1::from_carried(&j)
    };
    assert!(matches!(with(&|j| j.tokenizer_id = h(1)), Err(Shell(_))));
    assert!(matches!(with(&|j| j.prompt_token_ids_hash = h(1)), Err(Shell(_))));
    assert!(matches!(with(&|j| j.prompt_tokens = 1), Err(Shell(_))));
    assert!(matches!(with(&|j| j.decode_token_limit = 1), Err(Shell(_))));
    assert!(matches!(with(&|j| j.max_context_tokens = 1), Err(Shell(_))));
    assert!(matches!(with(&|j| j.sampling_seed[0] = 1), Err(Shell(_))));
    assert!(matches!(with(&|j| j.temperature_q = 1), Err(Shell(_))));
    assert!(matches!(with(&|j| j.decode = Some(DecodeConfigV4::NOOP)), Err(PalwGenClaimErrorV1::NotATensorJob { .. })), "a decode config");
    assert!(matches!(with(&|j| j.tail = None), Err(PalwGenClaimErrorV1::NotATensorJob { .. })));
    // The id is total over whatever a struct holds: a malformed version-10 job has an id, under its own key.
    let mut malformed = good.clone();
    malformed.tail = None;
    assert_ne!(fp_job_id_v3(&malformed), job.id());
    assert_eq!(fp_job_id_v3(&malformed), fp_job_id_v3(&malformed));
}

#[test]
fn the_work_identity_is_the_class_the_tail_and_the_bond() {
    let (job, _) = image_job(PALW_FP_PRIVACY_PUBLIC_DA, false);
    let (class, tail, bond) = (job.envelope.class_id, job.tail(), job.envelope.executor_bond);
    let id = palw_gen_work_id_v1(&class, &tail, &bond);
    // The same inference under another nonce or anchor is the same work; another bond's run is another.
    let mut renonced = job.clone();
    renonced.envelope.job_nonce = [1; 32];
    renonced.envelope.anchor_daa += 1;
    assert_eq!(palw_gen_work_id_v1(&class, &renonced.tail(), &renonced.envelope.executor_bond), id);
    let other_bond = TransactionOutpoint::new(TransactionId::from_bytes([8; 64]), 3);
    assert_ne!(palw_gen_work_id_v1(&class, &tail, &other_bond), id);
    let mut reseeded = tail.clone();
    reseeded.seed[0] ^= 1;
    assert_ne!(palw_gen_work_id_v1(&class, &reseeded, &bond), id, "another seed is another image");
    assert_ne!(palw_gen_work_id_v1(&h(0xC2), &tail, &bond), id, "another class");
}

// ---------------------------------------------------------------------------------------------
// The commitment's own rules
// ---------------------------------------------------------------------------------------------

#[test]
fn an_honest_tensor_commitment_passes_and_its_roots_are_its_parts() {
    for (job, ids) in [image_job(PALW_FP_PRIVACY_PUBLIC_DA, true), image_job(PALW_FP_PRIVACY_PANEL_DA, false), (embedding_job(), vec![])] {
        let ids = if job.envelope.privacy_mode == PALW_FP_PRIVACY_PANEL_DA { vec![] } else { ids };
        let payload = payload_of(&job, ids);
        assert_eq!(check(&payload), Ok(job.clone()));
        let c = &payload.commitment;
        assert_eq!(c.execution_root, palw_gen_commitment_execution_root_v1(c, &job));
        assert_eq!(
            c.execution_root,
            kaspa_consensus_core::palw_gen_close_v1::palw_gen_tensor_execution_root_v1(&job.id(), &c.job.class_id, c.work_leaves, &c.trace_root, &c.output_root)
        );
        assert_eq!((c.decode_tokens_executed, c.stop_reason, c.trace_chunk_count, c.trace_retention_daa), (0, PalwFpStopReasonV3::ExactBudgetReached, 1, 0));
        // The claim id is the lane's: the hash of the whole commitment.
        assert_eq!(payload.claim_id(), fp_claim_id_v3(&payload.commitment));
    }
}

#[test]
fn every_way_a_tensor_commitment_is_not_the_canonical_one_is_refused_by_name() {
    use PalwGenClaimErrorV1 as E;
    let (job, ids) = image_job(PALW_FP_PRIVACY_PUBLIC_DA, true);
    let good = payload_of(&job, ids.clone());
    assert!(check(&good).is_ok());
    // The payload and the envelope.
    assert_eq!(check(&edited(good.clone(), |p| p.version = 9)), Err(E::PayloadVersion { got: 9, expected: PALW_FP_V3_VERSION }));
    assert_eq!(
        palw_fp_gen_claim_check_v1(&good, Some(h(1)), true, PALW_FP_STRUCTURAL_WORK_LEAVES_CAP, FORM),
        Err(E::NetworkDomainMismatch),
        "another network's job"
    );
    assert!(palw_fp_gen_claim_check_v1(&good, None, true, PALW_FP_STRUCTURAL_WORK_LEAVES_CAP, FORM).is_ok(), "the height-free door holds no domain");
    assert_eq!(check(&edited(good.clone(), |p| p.commitment.job.privacy_mode = 0)), Err(E::PrivacyModeNotOffered(0)));
    assert_eq!(check(&edited(good.clone(), |p| p.commitment.job.privacy_mode = 3)), Err(E::PrivacyModeNotOffered(3)));
    assert_eq!(
        palw_fp_gen_claim_check_v1(&edited(good.clone(), |p| p.commitment.job.privacy_mode = PALW_FP_PRIVACY_PANEL_DA), Some(net()), false, PALW_FP_STRUCTURAL_WORK_LEAVES_CAP, FORM),
        Err(E::PanelDaNotArmed)
    );
    assert_eq!(check(&edited(good.clone(), |p| p.commitment.job.prompt_mode = 1)), Err(E::PromptModeNotOffered(1)));
    assert_eq!(check(&edited(good.clone(), |p| p.commitment.job.executor_pubkey.clear())), Err(E::MissingPublicKey));
    assert_eq!(
        check(&edited(good.clone(), |p| p.signature.truncate(10))),
        Err(E::SignatureLength { got: 10, expected: MLDSA87_SIGNATURE_LEN })
    );
    // The commitment's fields (§I.4.2).
    assert!(matches!(check(&edited(good.clone(), |p| p.commitment.decode_tokens_executed = 1)), Err(E::CommitmentNotCanonical(_))));
    assert!(matches!(check(&edited(good.clone(), |p| p.commitment.stop_reason = PalwFpStopReasonV3::EndOfGeneration)), Err(E::CommitmentNotCanonical(_))));
    assert!(matches!(check(&edited(good.clone(), |p| p.commitment.schedule_root = h(1))), Err(E::CommitmentNotCanonical(_))));
    assert!(matches!(check(&edited(good.clone(), |p| p.commitment.trace_manifest_root = h(1))), Err(E::CommitmentNotCanonical(_))));
    assert!(matches!(check(&edited(good.clone(), |p| p.commitment.trace_chunk_count = 2)), Err(E::CommitmentNotCanonical(_))));
    assert!(matches!(check(&edited(good.clone(), |p| p.commitment.trace_retention_daa = 1)), Err(E::CommitmentNotCanonical(_))));
    assert_eq!(check(&edited(good.clone(), |p| p.commitment.work_leaves = 0)), Err(E::ZeroWorkLeaves));
    assert_eq!(
        palw_fp_gen_claim_check_v1(&good, Some(net()), true, LEAVES - 1, FORM),
        Err(E::WorkLeavesAboveCap { got: LEAVES, max: LEAVES - 1 }),
        "the class's ladder is the walk's bound"
    );
    for (name, edit) in [
        ("trace_root", (|p: &mut PalwFpCommitmentTxPayloadV3| p.commitment.trace_root = Hash64::default()) as fn(&mut PalwFpCommitmentTxPayloadV3)),
        ("output_root", |p| p.commitment.output_root = Hash64::default()),
        ("execution_root", |p| p.commitment.execution_root = Hash64::default()),
    ] {
        assert_eq!(check(&edited(good.clone(), edit)), Err(E::ZeroRoot(name)));
    }
    // The execution root is the tensor execution root of the commitment's own parts.
    assert_eq!(check(&edited(good.clone(), |p| p.commitment.trace_root = h(99))), Err(E::ExecutionRootNotItsParts));
    assert_eq!(check(&edited(good.clone(), |p| p.commitment.output_root = h(99))), Err(E::ExecutionRootNotItsParts));
    assert_eq!(check(&edited(good.clone(), |p| p.commitment.work_leaves += 1)), Err(E::ExecutionRootNotItsParts));
    assert_eq!(check(&edited(good.clone(), |p| p.commitment.execution_root = h(99))), Err(E::ExecutionRootNotItsParts));
    // The body's text commitments: an id list is the zero hash and a count of 0, and nothing else.
    let with_body = |edit: &dyn Fn(&mut PalwGenImageBodyV1)| {
        let mut j = job.clone();
        let PalwGenBodyV1::Image(b) = &mut j.body else { unreachable!() };
        edit(b);
        check(&payload_of(&j, ids.clone()))
    };
    assert!(matches!(with_body(&|b| b.prompt_tokens = 0), Err(E::EmptyIdsEncoding { what: "prompt", .. })));
    assert!(matches!(with_body(&|b| b.negative_token_ids_hash = Hash64::default()), Err(E::EmptyIdsEncoding { what: "negative prompt", .. })));
}

#[test]
fn a_public_claims_ids_are_its_prompt_then_its_negative_prompt_and_a_private_claims_are_none() {
    use PalwGenClaimErrorV1 as E;
    let (job, ids) = image_job(PALW_FP_PRIVACY_PUBLIC_DA, true);
    assert_eq!(ids.len(), 5, "three prompt ids, two negative");
    assert!(check(&payload_of(&job, ids.clone())).is_ok());
    assert_eq!(check(&payload_of(&job, ids[..4].to_vec())), Err(E::IdsCount { got: 4, declared: 5 }));
    assert_eq!(check(&payload_of(&job, vec![])), Err(E::IdsCount { got: 0, declared: 5 }), "a public claim carries its ids");
    let mut swapped = ids.clone();
    swapped.swap(0, 4);
    assert_eq!(check(&payload_of(&job, swapped)), Err(E::IdsNotTheJobs { what: "prompt" }));
    let mut other_negative = ids.clone();
    other_negative[4] ^= 1;
    assert_eq!(check(&payload_of(&job, other_negative)), Err(E::IdsNotTheJobs { what: "negative prompt" }));
    // The split is at the prompt's count.
    assert_eq!(palw_gen_split_ids_v1(&ids, 3), (&ids[..3], &ids[3..]));
    assert_eq!(palw_gen_split_ids_v1(&ids, 99), (&ids[..], &ids[5..]), "a count past the list splits nothing out of range");
    // Under PanelDa the payload carries no ids, and a payload that does is refused.
    let (private, private_ids) = image_job(PALW_FP_PRIVACY_PANEL_DA, true);
    assert!(check(&payload_of(&private, vec![])).is_ok());
    assert_eq!(check(&payload_of(&private, private_ids)), Err(E::PanelDaPayloadCarriesIds(5)));
    // An embedding of an image carries no ids at all.
    assert!(check(&payload_of(&embedding_job(), vec![])).is_ok());
}

// ---------------------------------------------------------------------------------------------
// The doors
// ---------------------------------------------------------------------------------------------

#[test]
fn the_isolation_door_admits_a_tensor_claim_only_where_the_ruleset_carries_the_fence() {
    let (job, ids) = image_job(PALW_FP_PRIVACY_PUBLIC_DA, false);
    let payload = payload_of(&job, ids);
    let bytes = borsh::to_vec(&payload).unwrap();
    assert!(palw_fp_payload_is_gen_v1(&bytes), "the job's version word is at bytes 2..4");
    // A V3 payload is not one.
    let v3 = borsh::to_vec(&edited(payload.clone(), |p| p.commitment.job.version = PALW_FP_V3_VERSION)).unwrap();
    assert!(!palw_fp_payload_is_gen_v1(&v3));
    let door = |gen_door: bool, bytes: &[u8]| {
        validate_palw_fp_commitment_tx_gen_door_v1(
            bytes,
            false,
            FORM,
            PALW_FP_STRUCTURAL_WORK_LEAVES_CAP,
            PalwFpDecodeRulesV1::Scheduled,
            gen_door,
        )
    };
    // No fence: the lane's own door refuses version 10 by name, as a build without the fence refuses its bytes.
    assert_eq!(door(false, &bytes), Err(PalwFpV3Error::UnsupportedVersion { got: PALW_FP_GEN_VERSION, expected: PALW_FP_V3_VERSION }));
    // The fence carried: the tensor door's rules, height-free (a private claim needs the mode's arming at the door).
    assert_eq!(door(true, &bytes), Ok(()));
    let (private, _) = image_job(PALW_FP_PRIVACY_PANEL_DA, false);
    let private_bytes = borsh::to_vec(&payload_of(&private, vec![])).unwrap();
    assert!(matches!(door(true, &private_bytes), Err(PalwFpV3Error::TensorClaim(why)) if why.contains("PanelDa")), "PanelDa is not armed at this door");
    assert_eq!(
        validate_palw_fp_gen_commitment_tx_v1(&private_bytes, true, FORM, PALW_FP_STRUCTURAL_WORK_LEAVES_CAP),
        Ok(()),
        "armed, the same bytes are admitted"
    );
    // The tensor door's refusals are named by the tensor lane.
    let bad_root = borsh::to_vec(&edited(payload.clone(), |p| p.commitment.trace_root = h(99))).unwrap();
    assert!(matches!(door(true, &bad_root), Err(PalwFpV3Error::TensorClaim(why)) if why.contains("execution_root")));
    // A payload whose job version word is 10 but that does not decode is the tensor door's; bytes with any other
    // version word are the lane's own door's (it says what it always said).
    let mut torn = bytes.clone();
    torn.truncate(40);
    assert!(matches!(door(true, &torn), Err(PalwFpV3Error::TensorClaim(why)) if why.contains("decode")), "a torn tensor payload");
    assert_eq!(door(true, b"junk"), Err(PalwFpV3Error::PayloadUndecodable), "bytes that are no tensor claim go to the lane's door");
    // Every other payload goes to the lane's own door, fence or not (an FP payload is untouched).
    let v3_door_a = door(false, &v3);
    let v3_door_b = door(true, &v3);
    assert_eq!(v3_door_a, v3_door_b, "a non-tensor payload meets the same door whichever way the fence reads");
}

#[test]
fn the_header_context_door_refuses_a_tensor_claim_below_the_fence() {
    let (job, ids) = image_job(PALW_FP_PRIVACY_PUBLIC_DA, false);
    let bytes = borsh::to_vec(&payload_of(&job, ids)).unwrap();
    let why = palw_fp_gen_refusal_at_v1(&bytes, false).expect("refused below the fence");
    assert!(why.contains("palw_fp_job_v5"), "{why}");
    assert_eq!(palw_fp_gen_refusal_at_v1(&bytes, true), None, "admitted from the fence");
    // A payload that is not a tensor claim is never this door's.
    let v3 = borsh::to_vec(&edited(payload_of(&job, vec![3, 1, 4]), |p| p.commitment.job.version = PALW_FP_V3_VERSION)).unwrap();
    assert_eq!(palw_fp_gen_refusal_at_v1(&v3, false), None);
    assert_eq!(palw_fp_gen_refusal_at_v1(b"", false), None);
}

fn ladder(_: &Hash64) -> u64 {
    PALW_FP_STRUCTURAL_WORK_LEAVES_CAP
}

#[test]
fn the_walk_turns_exactly_the_admissible_payloads_into_the_object_the_fold_reads() {
    let (job, ids) = image_job(PALW_FP_PRIVACY_PUBLIC_DA, true);
    let good = payload_of(&job, ids.clone());
    let (private, _) = image_job(PALW_FP_PRIVACY_PANEL_DA, false);
    let private_payload = payload_of(&private, vec![]);
    let bad_root = edited(good.clone(), |p| p.commitment.trace_root = h(99));
    let other_net = edited(good.clone(), |p| p.commitment.job.network_domain = h(1));
    let txs = vec![
        tx_of(&good),
        tx_of(&bad_root),
        tx_of(&other_net),
        tx_of(&private_payload),
        // Bytes whose job version word is 10 and that do not decode: the tensor walk's skip.
        Transaction::new(0, vec![], vec![], 0, SUBNETWORK_ID_PALW_FP_COMMITMENT, 0, torn_payload(&good)),
        // Bytes that are no tensor claim, and a native transaction: not the tensor walk's at all.
        Transaction::new(0, vec![], vec![], 0, SUBNETWORK_ID_PALW_FP_COMMITMENT, 0, b"not a payload".to_vec()),
        Transaction::new(0, vec![], vec![], 0, SUBNETWORK_ID_NATIVE, 0, vec![]),
    ];
    let accept_all = |_: &[u8], _: &[u8], _: &[u8], _: &[u8]| true;
    let out = palw_fp_gen_objects_from_accepted_txs_v1(&txs, net(), true, ladder, FORM, accept_all);
    assert_eq!(out.objects.len(), 2, "the two honest claims: {:?}", out.skipped);
    assert_eq!(out.skipped.len(), 3, "the bad root, the other network and the torn tensor payload; junk and a native transaction are not the walk's");
    // The object is what the fold reads.
    let first = &out.objects[0];
    assert_eq!(first.carrier, txs[0].id());
    let PalwConsensusObjectV2::GenTensorCommitted {
        claim, class_id, bond, executor_pubkey, work_leaves, prompt_token_ids, trace_root, output_root, execution_root, job_pin, job: read,
    } = &first.object
    else {
        panic!("a tensor claim's object")
    };
    assert_eq!(*claim, good.claim_id());
    assert_eq!(*class_id, job.envelope.class_id);
    assert_eq!(bond.0, job.envelope.executor_bond);
    assert_eq!(*executor_pubkey, job.envelope.executor_pubkey);
    assert_eq!(*work_leaves, LEAVES);
    assert_eq!(*prompt_token_ids, ids, "a public claim's prompt ids then its negative ids");
    assert_eq!((*trace_root, *output_root, *execution_root), (good.commitment.trace_root, good.commitment.output_root, good.commitment.execution_root));
    assert_eq!(*job_pin, kaspa_consensus_core::palw_fp_execution_v3::palw_fp_job_pin_v1(&good.commitment));
    assert_eq!(**read, job);
    let PalwConsensusObjectV2::GenTensorCommitted { prompt_token_ids: private_ids, job: private_job, .. } = &out.objects[1].object else {
        panic!("a private claim's object")
    };
    assert!(private_ids.is_empty(), "PanelDa carries none");
    assert_eq!(**private_job, private);
    // A signature that does not verify under the carried key skips its claim.
    let reject_all = |_: &[u8], _: &[u8], _: &[u8], _: &[u8]| false;
    let none = palw_fp_gen_objects_from_accepted_txs_v1(&txs[..1], net(), true, ladder, FORM, reject_all);
    assert!(none.objects.is_empty());
    assert_eq!(none.skipped.len(), 1);
    // PanelDa not armed at the accepting block: the private claim is skipped, the public one stands.
    let unarmed = palw_fp_gen_objects_from_accepted_txs_v1(&txs, net(), false, ladder, FORM, accept_all);
    assert_eq!(unarmed.objects.len(), 1);
    // The class's ladder is the walk's bound.
    let narrow = |_: &Hash64| LEAVES - 1;
    assert!(palw_fp_gen_objects_from_accepted_txs_v1(&txs[..1], net(), true, narrow, FORM, accept_all).objects.is_empty());
    // The lane's own walk does not see a tensor claim: it skips the payload as not stateless-admissible.
    let params = kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptParamsV3::new(
        kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_RECEIPT_V3,
        8,
        4,
        64,
        64,
        100,
        100,
        10,
    )
    .expect("params");
    let fp_walk = kaspa_consensus_core::palw_fp_objects_v3::palw_fp_objects_from_accepted_txs_v3(
        &txs[..1],
        net(),
        &params,
        kaspa_consensus_core::BlockHash::default(),
        FORM,
        accept_all,
    );
    assert!(fp_walk.objects.is_empty(), "the lane's own walk skips a tensor claim: it is not a version it reads");
}

/// The embedding image's claim: an object with no ids, whose job reads back whole.
#[test]
fn an_embedding_claim_is_an_object_with_no_ids() {
    let job = embedding_job();
    let payload = payload_of(&job, vec![]);
    let accept_all = |_: &[u8], _: &[u8], _: &[u8], _: &[u8]| true;
    let out = palw_fp_gen_objects_from_accepted_txs_v1(&[tx_of(&payload)], net(), true, ladder, FORM, accept_all);
    let [one] = &out.objects[..] else { panic!("one object: {:?}", out.skipped) };
    let PalwConsensusObjectV2::GenTensorCommitted { prompt_token_ids, job: read, .. } = &one.object else { panic!("a tensor claim's object") };
    assert!(prompt_token_ids.is_empty());
    assert_eq!(**read, job);
    assert_eq!(job.text_commitments(), (Hash64::default(), 0, Hash64::default(), 0));
}
