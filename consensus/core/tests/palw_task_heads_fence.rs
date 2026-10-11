//! **The task-head profile and its fence `palw_task_heads_v1`** (HFX, 2026-10-08; `docs/design/palw/tir/task-heads-profile-v1.md`).
//!
//! * The fence is dormant on every ruleset and **this build refuses arming it** (`validate_palw_v2`); set on a `Params` built directly (a
//!   drill's entry) it moves the ruleset and the schedule and never the identity, and `Some(never())` is absence. What the
//!   full-activation release will require beyond the refusal (`palw_task_heads_v1_arming_preconditions`): `palw_gen_v1` at or below it,
//!   a mirror that agrees, this build's head set.
//! * **The A-2 split class** (the Lead's critical condition): `palw_gen_v1` is ARMED on testnet-12, so the int-12 build decodes tag 68 and
//!   FP job version 10 — but not the appended `Head` variants. A mirror of int-12's two enums, below, shows exactly which bytes it cannot
//!   decode; the one predicate (`palw_object_needs_task_heads_v1`) names exactly those objects (a mixed set: a legacy registration and a
//!   `Head` one); the isolation gate reads them as int-12 does (tolerated on an audit-armed ruleset, refused elsewhere); and a class whose
//!   profile BYTE is 6 with an older offers variant — bytes int-12 decodes — keeps int-12's own verdict at admission
//!   (`Profile(6)`), never the drop.
//! * The decode rule (`HEAD_DECODE_V1`) is a pure function of the verified output.

use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_consensus_core::config::params::{ForkActivation, Params, palw_t12_shipped_params};
use kaspa_consensus_core::palw_gen_class_v1::{
    OutputSpecV1, PALW_GEN_CLASS_VERSION_V1, PalwGenAdmissionCarriageV1, PalwGenClassErrorV1, PalwGenClassV1,
    PalwGenEmbeddingOffersV1, PalwGenImageOffersV1, PalwGenOffersV1, PalwGenProfileOffersV1, palw_gen_class_preflight_v1,
    palw_gen_class_preflight_with_heads_v1,
};
use kaspa_consensus_core::palw_gen_job_v1::{
    PalwGenBodyV1, PalwGenEmbeddingBodyV1, PalwGenEmbeddingInputV1, PalwGenImageBodyV1, PalwGenJobV1, PalwJobEnvelopeV1,
};
use kaspa_consensus_core::palw_gen_v1::{PalwGenFenceV1, PalwGenProfileV1};
use kaspa_consensus_core::palw_lifecycle_objects_v2::{
    PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxError, PalwLifecycleTxPayloadV2, validate_palw_lifecycle_tx,
};
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2, PalwPwuRuleV2};
use kaspa_consensus_core::palw_task_heads_v1::*;
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use kaspa_hashes::Hash64;

/// A height no testnet-12 fence uses, above `palw_gen_v1` (DAA 5,300 on testnet-12).
const AT: u64 = 9_999_991;

fn ids(p: &Params) -> (String, String, String) {
    (p.consensus_params_id().to_string(), p.consensus_identity_id().to_string(), p.consensus_schedule_id().to_string())
}

fn armed(at: ForkActivation) -> Params {
    let mut p = palw_t12_shipped_params();
    (PALW_DRILL_TASK_HEADS_ENTRY.set)(&mut p, Some(at));
    p
}

#[test]
fn the_fence_is_dormant_everywhere_and_armed_it_moves_the_ruleset_never_the_identity() {
    let base = palw_t12_shipped_params();
    assert!(
        base.palw_task_heads_v1.is_none() && base.palw_task_heads_v1_fence().is_none() && !base.palw_task_heads_active_at(u64::MAX)
    );
    assert!(base.palw_fences_v1().contains(&("palw_task_heads_v1", None)), "the exhaustive fence list names it");
    assert!(base.validate_palw_task_heads_v1().is_ok());
    let a = armed(ForkActivation::new(AT));
    let refused = a.validate_palw_v2().expect_err("this build refuses arming the dormant fence");
    assert!(refused.to_string().contains("palw_task_heads_v1 cannot be armed"), "{refused}");
    a.palw_task_heads_v1_arming_preconditions().unwrap_or_else(|e| panic!("testnet-12 past palw_gen_v1 meets the preconditions: {e}"));
    assert!(a.palw_task_heads_v1_fence().is_some() && !a.palw_task_heads_active_at(AT - 1) && a.palw_task_heads_active_at(AT));
    let (b, m) = (ids(&base), ids(&a));
    assert_ne!(m.0, b.0, "the ruleset names the fence and its value");
    assert_eq!(m.1, b.1, "two builds that differ only about a FUTURE height stay peers");
    assert_ne!(m.2, b.2, "the schedule reports it");
    // `Some(never())` is absence for the identity (the normaliser collapses it whole), and a dormant value validates.
    let never = armed(ForkActivation::never());
    assert_eq!(never.consensus_identity_id(), base.consensus_identity_id(), "never() collapses");
    never.validate_palw_v2().unwrap_or_else(|e| panic!("a dormant value validates: {e}"));
    assert!(!never.palw_task_heads_active_at(u64::MAX - 1));
    // Its value is part of the ruleset: another head set is another ruleset, and a ruleset this build refuses.
    let mut other = a.clone();
    other.palw_task_heads_v1 =
        Some(PalwTaskHeadsFenceV1 { head_set_id: Hash64::from_bytes([7; 64]), ..a.palw_task_heads_v1.unwrap() });
    assert_ne!(ids(&other).0, m.0);
    assert!(other.palw_task_heads_v1_arming_preconditions().is_err());
}

#[test]
fn the_fence_is_refused_without_its_prerequisite_or_its_mirror() {
    // Below `palw_gen_v1`: a head class is a generative class.
    let mut p = palw_t12_shipped_params();
    let gen_at = p.palw_gen_v1.expect("testnet-12 arms palw_gen_v1").activation.daa_score();
    (PALW_DRILL_TASK_HEADS_ENTRY.set)(&mut p, Some(ForkActivation::new(gen_at - 1)));
    assert!(p.palw_task_heads_v1_arming_preconditions().is_err(), "below palw_gen_v1");
    // A field set without its mirror: refused by both, and the mirror's refusal comes first.
    let mut p = palw_t12_shipped_params();
    p.palw_task_heads_v1 = Some(PalwTaskHeadsFenceV1::testnet12_v1(ForkActivation::new(AT)));
    let e = p.validate_palw_task_heads_v1().expect_err("the mirror disagrees until synced");
    assert!(e.to_string().contains("mirror"), "{e}");
    assert!(p.palw_task_heads_v1_arming_preconditions().is_err());
    p.sync_palw_task_heads_v1();
    p.palw_task_heads_v1_arming_preconditions().expect("synced, past palw_gen_v1");
    assert!(p.validate_palw_task_heads_v1().is_err(), "and still refused: dormant in this build");
}

/// **int-12's two enums, as that build declares them** (`palw_gen_class_v1::PalwGenProfileOffersV1` and `palw_gen_job_v1::PalwGenBodyV1`
/// before the `Head` variants were appended), to show which bytes it cannot decode.
mod int12 {
    use super::*;
    #[derive(Debug, BorshSerialize, BorshDeserialize)]
    pub enum ProfileOffers {
        None,
        Image(PalwGenImageOffersV1),
        Embedding(PalwGenEmbeddingOffersV1),
    }
    #[derive(Debug, BorshSerialize, BorshDeserialize)]
    pub enum Body {
        Image(PalwGenImageBodyV1),
        Embedding(PalwGenEmbeddingBodyV1),
    }
}

fn head_offers() -> PalwGenHeadOffersV1 {
    PalwGenHeadOffersV1 {
        task: PALW_HEAD_TASK_SEQUENCE_V1,
        problem: PALW_HEAD_PROBLEM_SINGLE_LABEL_V1,
        labels: 3,
        label_map_root: palw_head_label_map_root_v1(&["negative", "neutral", "positive"]),
        pair_separator: vec![],
        entailment_label: None,
        position_scalar: None,
    }
}

fn offers(profile: PalwGenProfileOffersV1) -> PalwGenOffersV1 {
    PalwGenOffersV1 {
        steps: vec![],
        scalars: vec![],
        max_prompt_tokens: 10,
        max_negative_tokens: 0,
        images: vec![],
        max_source_tokens: 0,
        source_token_floor: 0,
        forced_prompt_prefix: vec![],
        profile,
    }
}

fn class(profile: u8, offers_profile: PalwGenProfileOffersV1) -> PalwGenClassV1 {
    PalwGenClassV1 {
        version: PALW_GEN_CLASS_VERSION_V1,
        profile,
        pipeline: vec![],
        programs: vec![],
        layouts: vec![],
        output: OutputSpecV1::embedding_i32(1, 3, 0, false),
        offers: offers(offers_profile),
        tokenizer_id: Hash64::from_bytes([0xC1; 64]),
    }
}

fn registration(class: PalwGenClassV1) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::ClassRegisteredGenV1 {
        class_id: class.class_id(&Hash64::default()),
        artifact_root: Hash64::default(),
        slash_value_per_pwu: 1,
        pwu_rule: PalwPwuRuleV2::MaxPerAttempt(1),
        initial_target: 1,
        share_permille: 0,
        activation_daa: 0,
        admission: Box::new(PalwGenAdmissionCarriageV1 {
            class,
            registrant_bond: PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_bytes([0; 64]), 0)),
            signature: vec![],
        }),
    }
}

fn job(body: PalwGenBodyV1) -> PalwGenJobV1 {
    PalwGenJobV1 {
        version: 1,
        envelope: PalwJobEnvelopeV1 {
            network_domain: Hash64::default(),
            class_id: Hash64::default(),
            executor_bond: TransactionOutpoint::new(TransactionId::from_bytes([0; 64]), 0),
            executor_pubkey: vec![],
            operator_id: Hash64::default(),
            anchor_block: Hash64::default(),
            anchor_daa: 0,
            job_nonce: [0; 32],
            privacy_mode: 1,
            prompt_mode: 0,
        },
        seed: [0; 32],
        body,
    }
}

#[test]
fn the_appended_variants_are_exactly_the_bytes_int12_cannot_decode() {
    // Offers: the legacy variants decode on int-12 to the same bytes; `Head` (variant 3) does not decode at all.
    for (o, int12_decodes) in [
        (PalwGenProfileOffersV1::None, true),
        (PalwGenProfileOffersV1::Embedding(PalwGenEmbeddingOffersV1 { pooling: 1, dims: vec![3] }), true),
        (PalwGenProfileOffersV1::Head(head_offers()), false),
    ] {
        let bytes = borsh::to_vec(&o).unwrap();
        match borsh::from_slice::<int12::ProfileOffers>(&bytes) {
            Ok(old) => {
                assert!(int12_decodes, "{o:?}");
                assert_eq!(borsh::to_vec(&old).unwrap(), bytes, "the legacy encoding is unchanged");
            }
            Err(_) => assert!(!int12_decodes, "{o:?}"),
        }
    }
    assert_eq!(borsh::to_vec(&PalwGenProfileOffersV1::Head(head_offers())).unwrap()[0], 3, "Head is offers variant 3");
    // Bodies: likewise, `Head` is variant 2.
    let text = PalwGenEmbeddingInputV1::Text { token_ids_hash: Hash64::from_bytes([1; 64]), tokens: 4 };
    let head_body =
        PalwGenBodyV1::Head(PalwGenHeadBodyV1 { input: text.clone(), task: PALW_HEAD_TASK_SEQUENCE_V1, position: 0, output: 3 });
    let emb_body = PalwGenBodyV1::Embedding(PalwGenEmbeddingBodyV1 { input: text, pooling: 1, dims: 3, output: 3 });
    assert_eq!(borsh::to_vec(&head_body).unwrap()[0], 2, "Head is body variant 2");
    assert!(borsh::from_slice::<int12::Body>(&borsh::to_vec(&head_body).unwrap()).is_err());
    let emb = borsh::to_vec(&emb_body).unwrap();
    assert_eq!(borsh::to_vec(&borsh::from_slice::<int12::Body>(&emb).unwrap()).unwrap(), emb);
    // The profile tag: 6, not one of `palw_gen_v1`'s (its fingerprint iterates `ALL`), resolved only with the head fence.
    assert_eq!(PalwGenProfileV1::Head as u8, 6);
    assert!(!PalwGenProfileV1::ALL.contains(&PalwGenProfileV1::Head));
    assert_eq!(PalwGenProfileV1::from_tag(6), None, "int-12's resolution");
    assert_eq!(PalwGenProfileV1::from_tag_with_heads(6), Some(PalwGenProfileV1::Head));
}

/// **The mixed-verdict test**: one set of objects, each held to int-12's reading — the predicate names exactly the objects carrying a
/// `Head` variant; the isolation gate tolerates those on an audit-armed ruleset and refuses them elsewhere, exactly as int-12's
/// undecodable branch does; the others take the gate's ordinary path on both rulesets.
#[test]
fn a_mixed_set_is_read_object_by_object_as_int12_reads_it() {
    let legacy = registration(class(
        PalwGenProfileV1::Embedding as u8,
        PalwGenProfileOffersV1::Embedding(PalwGenEmbeddingOffersV1 { pooling: 1, dims: vec![3] }),
    ));
    let head = registration(class(PalwGenProfileV1::Head as u8, PalwGenProfileOffersV1::Head(head_offers())));
    // Profile byte 6 with an older offers variant: bytes int-12 decodes, so NOT dropped — int-12's admission refuses it as `Profile(6)`.
    let tag6_legacy_offers =
        registration(class(6, PalwGenProfileOffersV1::Embedding(PalwGenEmbeddingOffersV1 { pooling: 1, dims: vec![3] })));
    let text = PalwGenEmbeddingInputV1::Text { token_ids_hash: Hash64::from_bytes([1; 64]), tokens: 4 };
    let tensor = |body| PalwConsensusObjectV2::GenTensorCommitted {
        claim: Hash64::default(),
        class_id: Hash64::default(),
        bond: PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_bytes([0; 64]), 0)),
        executor_pubkey: vec![],
        work_leaves: 1,
        prompt_token_ids: vec![],
        trace_root: Hash64::default(),
        output_root: Hash64::default(),
        execution_root: Hash64::default(),
        job_pin: Hash64::default(),
        job: Box::new(job(body)),
    };
    let head_job = tensor(PalwGenBodyV1::Head(PalwGenHeadBodyV1 { input: text.clone(), task: 1, position: 0, output: 3 }));
    let emb_job = tensor(PalwGenBodyV1::Embedding(PalwGenEmbeddingBodyV1 { input: text, pooling: 1, dims: 3, output: 3 }));
    // A signed-expiry envelope (tag 108) is judged as the registration it wraps.
    let signed = |inner: &PalwConsensusObjectV2| PalwConsensusObjectV2::SignedRegistrationV1 {
        registration: Box::new(inner.clone()),
        valid_from_daa: 0,
        valid_until_daa: 1,
        fork_digest: kaspa_consensus_core::Hash::default(),
        signer: PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_bytes([0; 64]), 0)),
        signature: vec![],
    };
    let (signed_head, signed_legacy) = (signed(&head), signed(&legacy));
    let set = [
        ("legacy registration", &legacy, false),
        ("Head registration", &head, true),
        ("profile byte 6, legacy offers", &tag6_legacy_offers, false),
        ("Head tensor job", &head_job, true),
        ("embedding tensor job", &emb_job, false),
        ("signed envelope around a Head registration", &signed_head, true),
        ("signed envelope around a legacy registration", &signed_legacy, false),
    ];
    for (what, object, needs) in set {
        assert_eq!(palw_object_needs_task_heads_v1(object), needs, "{what}");
    }
    // The isolation gate on the two registrations a lifecycle carrier can hold.
    let payload = |o: &PalwConsensusObjectV2| {
        borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object: o.clone() }).unwrap()
    };
    assert_eq!(
        validate_palw_lifecycle_tx(&payload(&head), true),
        Ok(()),
        "tolerated on an audit-armed ruleset, as int-12 tolerates it"
    );
    assert_eq!(validate_palw_lifecycle_tx(&payload(&head), false), Err(PalwLifecycleTxError::Undecodable), "and refused elsewhere");
    for o in [&legacy, &tag6_legacy_offers] {
        assert_eq!(validate_palw_lifecycle_tx(&payload(o), false), validate_palw_lifecycle_tx(&payload(o), true), "the ordinary path");
    }
    // Profile byte 6 below the fence: int-12's own verdict at admission, by name.
    let gen_fence = PalwGenFenceV1::testnet12_v1(ForkActivation::new(1));
    let PalwConsensusObjectV2::ClassRegisteredGenV1 { admission, .. } = &tag6_legacy_offers else { unreachable!() };
    assert_eq!(palw_gen_class_preflight_v1(&admission.class, &gen_fence).unwrap_err(), PalwGenClassErrorV1::Profile(6));
    // Past it, tag 6 is the `Head` profile (this class is malformed in other ways, and refused for those — not as an unknown profile).
    let heads = PalwTaskHeadsFenceV1::testnet12_v1(ForkActivation::new(1));
    let PalwConsensusObjectV2::ClassRegisteredGenV1 { admission, .. } = &head else { unreachable!() };
    let e = palw_gen_class_preflight_with_heads_v1(&admission.class, &gen_fence, Some(&heads)).unwrap_err();
    assert_ne!(e, PalwGenClassErrorV1::Profile(6), "{e}");
    assert_eq!(palw_gen_class_preflight_v1(&admission.class, &gen_fence).unwrap_err(), PalwGenClassErrorV1::Profile(6));
}

#[test]
fn the_decode_rule_is_a_pure_function_of_the_verified_output() {
    assert_eq!(palw_head_argmax_v1(&[3, 7, 7, -1]), 1, "the smallest index among the maxima");
    assert_eq!(palw_head_decode_v1(PALW_HEAD_PROBLEM_SINGLE_LABEL_V1, &[3, 7, 7]), Some(PalwHeadDecisionV1::Label(1)));
    assert_eq!(palw_head_decode_v1(PALW_HEAD_PROBLEM_MULTI_LABEL_V1, &[3, 0, -2, 1]), Some(PalwHeadDecisionV1::Labels(vec![0, 3])));
    assert_eq!(palw_head_decode_v1(PALW_HEAD_PROBLEM_REGRESSION_V1, &[-5]), Some(PalwHeadDecisionV1::Value(-5)));
    assert_eq!(palw_head_decode_v1(PALW_HEAD_PROBLEM_SINGLE_LABEL_V1, &[]), None);
    // A span: the context starts after the question; ties go to the smallest (s, e).
    let start = [9, 0, 5, 1, 5];
    let end = [9, 0, 0, 6, 6];
    assert_eq!(palw_head_best_span_v1(&start, &end, 2, 5, 3), Some((2, 3)));
    assert_eq!(palw_head_best_span_v1(&start, &end, 5, 5, 3), None);
    // The label map's root binds every string and its order.
    let a = palw_head_label_map_root_v1(&["a", "b"]);
    assert_ne!(a, palw_head_label_map_root_v1(&["b", "a"]));
    assert_ne!(a, palw_head_label_map_root_v1(&["ab"]));
    assert_eq!(palw_task_heads_head_set_id_v1(), palw_task_heads_head_set_id_v1());
}
