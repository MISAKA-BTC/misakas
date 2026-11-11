use super::super::tests::{point, populated, tx};
use super::*;

#[test]
fn shared_manifest_seed_keeps_fees_separate_across_attempts() {
    let tx = tx();
    let at = point();
    let parent = Hash64::from_u64_word(91);
    let seed = ValidatedManifestRequestCarrierV1::prepare_shared_v1(&populated(&tx, 20), 8, parent, at.block, at, 2).unwrap().unwrap();
    assert_eq!(seed.carrier_v1(), tx.id());
    assert_eq!(seed.accepted_ordinal_v1(), 2);
    assert!(seed.candidate_at_v1(Hash64::default(), &at).is_err());
    assert!(seed.candidate_at_v1(parent, &PalwBlockContextV2 { daa_score: at.daa_score + 1, ..at }).is_err());
    let context = seed.context_v1();
    let other_parent = Hash64::from_u64_word(90);
    let other_seed =
        ValidatedManifestRequestCarrierV1::prepare_shared_v1(&populated(&tx, 20), 8, other_parent, at.block, at, 2).unwrap().unwrap();
    let other_candidate = other_seed.candidate_at_v1(other_parent, &at).unwrap();
    assert!(seed.project_after_candidate_v1(&other_candidate, 0).is_err());
    assert!(other_candidate.into_replay_seed_v1(parent).is_err());
    let initial = seed.candidate_at_v1(parent, &at).unwrap();
    let zero = seed.project_after_candidate_v1(&initial, 0).unwrap();
    assert_eq!(zero.collectible_at_v1(parent, &context, 20, 8), Ok(12));
    let signed = SignedManifestRequestV1::decode(&initial.request).unwrap();
    let packet = crate::palw_court_shared_material_intake_v1::batch::admission::protected::manifest_request_packet_id_v1(
        context.scope.claim,
        context.scope.reservation_scope,
    )
    .unwrap();
    let purpose = super::super::super::manifest_paid::manifest_request_charge_purpose_v1(signed.request.original, packet).unwrap();
    let execute = || {
        let mut candidate = seed.candidate_at_v1(parent, &at).unwrap();
        candidate.allocate_charge_v1(&context, purpose, 7).unwrap();
        assert!(seed.project_after_candidate_v1(&candidate, 0).is_err());
        let effect = seed.project_after_candidate_v1(&candidate, 7).unwrap();
        assert!(candidate.into_replay_seed_v1(parent).is_err());
        effect
    };
    let preview = execute();
    let actual = execute();
    assert_eq!(preview, actual);
    assert_eq!(actual.collectible_at_v1(parent, &context, 20, 8), Ok(5));
    assert_eq!(seed.candidate_at_v1(parent, &at).unwrap().remaining_fee_v1(), 12);
    let mut wrong_purpose = seed.candidate_at_v1(parent, &at).unwrap();
    wrong_purpose.allocate_charge_v1(&context, Hash64::from_u64_word(92), 7).unwrap();
    assert!(seed.project_after_candidate_v1(&wrong_purpose, 7).is_err());
    assert!(actual.collectible_at_v1(parent, &context, 21, 8).is_err());
    assert!(actual.collectible_at_v1(parent, &context, 20, 7).is_err());
    let source_changed = ManifestRequestCarrierContextV1 { carrier_block: Hash64::from_u64_word(93), ..context };
    assert!(actual.collectible_at_v1(parent, &source_changed, 20, 8).is_err());
    let totals = reconcile_manifest_request_fees_v1(&[preview], &[actual], parent, &at, 1).unwrap();
    assert_eq!(totals, ManifestRequestFeeTotalsV1 { actual_fee: 20, preceding_fee: 8, charge: 7, collectible_fee: 5 });
}

#[test]
fn manifest_fee_reconciliation_binds_each_carrier_not_just_totals() {
    let tx = tx();
    let at = point();
    let parent = Hash64::from_u64_word(91);
    let seed = ValidatedManifestRequestCarrierV1::prepare_shared_v1(&populated(&tx, 20), 8, parent, at.block, at, 2).unwrap().unwrap();
    let candidate = seed.candidate_at_v1(parent, &at).unwrap();
    let effect = seed.project_after_candidate_v1(&candidate, 0).unwrap();
    let rejects = |actual: &[ManifestRequestFeeV1], limit| {
        assert!(reconcile_manifest_request_fees_v1(&[effect], actual, parent, &at, limit).is_err());
    };
    rejects(&[], 1);
    rejects(&[effect], 0);
    for changed in [
        ManifestRequestFeeV1 { parent_root: Hash64::from_u64_word(92), ..effect },
        ManifestRequestFeeV1 { context: ManifestRequestCarrierContextV1 { accepted_ordinal: 3, ..effect.context }, ..effect },
        ManifestRequestFeeV1 {
            context: ManifestRequestCarrierContextV1 { carrier: Hash64::from_u64_word(93), ..effect.context },
            ..effect
        },
        ManifestRequestFeeV1 {
            context: ManifestRequestCarrierContextV1 { carrier_block: Hash64::from_u64_word(94), ..effect.context },
            ..effect
        },
        ManifestRequestFeeV1 { preceding_fee: 7, charge: 1, ..effect },
    ] {
        rejects(&[changed], 1);
    }
    let duplicate_carrier =
        ManifestRequestFeeV1 { context: ManifestRequestCarrierContextV1 { accepted_ordinal: 3, ..effect.context }, ..effect };
    assert!(reconcile_manifest_request_fees_v1(&[effect, duplicate_carrier], &[effect, duplicate_carrier], parent, &at, 2).is_err());
    let mut impossible = effect;
    impossible.collectible_fee += 1;
    assert!(reconcile_manifest_request_fees_v1(&[impossible], &[impossible], parent, &at, 1).is_err());
    let overflow = ManifestRequestFeeV1 { actual_fee: u64::MAX, preceding_fee: 0, charge: 0, collectible_fee: u64::MAX, ..effect };
    let other = ManifestRequestFeeV1 {
        context: ManifestRequestCarrierContextV1 { accepted_ordinal: 3, carrier: Hash64::from_u64_word(95), ..effect.context },
        ..overflow
    };
    assert!(reconcile_manifest_request_fees_v1(&[overflow, other], &[overflow, other], parent, &at, 2).is_err());
}
