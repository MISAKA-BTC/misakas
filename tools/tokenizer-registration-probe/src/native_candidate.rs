use crate::{
    native_bundle::{Action, Manifest},
    native_run::measurement_transaction,
};
use kaspa_consensus_core::{
    palw_state_v2::{
        PalwChainStateV2, PalwTransitionExtrasV1, TokenizerRegistrationFeeWorkingStateV1,
        palw_v2_apply_one_tokenizer_registration_owned_v1, palw_v2_pre_object_base_retained_v1,
    },
    palw_tokenizer_finish_derivation_cache_v1::TokenizerFinishDerivationCacheV1,
    palw_tokenizer_registration_carriage_v1::{
        TokenizerRegistrationCandidateWorkV1, TokenizerRegistrationCarrierContextV1, ValidatedTokenizerRegistrationCarrierV1,
        preflight_tokenizer_registration_source_body_v1, tokenizer_registration_parent_quote_v1,
    },
    tx::{PopulatedTransaction, ScriptPublicKey, UtxoEntry, ValidatedTransaction},
};
use std::{collections::BTreeMap, time::Instant};

pub const SOURCES: u64 = 780;

/// Exercise all physical copies through the same parent and accepting point.
/// Each rollback pass treats every copy as an abandoned one-carrier body.
/// The observed passes keep the completed prefix, as the actual fold will.
pub fn prepare(
    manifest: &Manifest,
    action: &Action,
    parent: &PalwChainStateV2,
    payload: &[u8],
    extras: &PalwTransitionExtrasV1,
    memo: &TokenizerFinishDerivationCacheV1<'_>,
    metrics: &mut BTreeMap<&str, u128>,
) -> PalwTransitionExtrasV1 {
    let policy = manifest.policy;
    assert_eq!(policy.maximum_source_blocks, SOURCES);
    assert_eq!(policy.source_budget_v1().unwrap().controls, 1);
    assert_eq!(memo.parent_root_v1(), action.parent_root);
    assert_eq!(action.burn, 0);
    assert_eq!(action.charge_id, None);

    let start = Instant::now();
    let policy_id = policy.id_v1().unwrap();
    let mut candidate = TokenizerRegistrationCandidateWorkV1::new(policy, true).unwrap();
    let mut authoritative = extras.clone();
    for ordinal in 0..SOURCES {
        let tx = measurement_transaction(action, ordinal as u32, payload.to_vec());
        if ordinal == 0 {
            assert_eq!(tx.id(), action.txid);
        }
        assert_eq!(preflight_tokenizer_registration_source_body_v1([&tx], &policy).unwrap().controls, 1);
        candidate.charge_v1(&tx, |id| tokenizer_registration_parent_quote_v1(parent, id)).unwrap();
        let populated = PopulatedTransaction::new(
            &tx,
            vec![UtxoEntry::new(action.paid.checked_add(10).unwrap(), ScriptPublicKey::default(), 0, false)],
        );
        let context = TokenizerRegistrationCarrierContextV1 {
            parent_root: action.parent_root,
            at: action.at,
            accepted_ordinal: usize::try_from(ordinal).unwrap(),
            network: manifest.network,
            policy: policy_id,
        };
        let carrier = ValidatedTokenizerRegistrationCarrierV1::prepare_shared_v1(
            &ValidatedTransaction::new(populated, action.paid),
            context,
            &policy,
            0,
        )
        .unwrap()
        .unwrap();
        authoritative.tokenizer_registration_carriers_v1.push((0, carrier));
    }
    assert_eq!(candidate.used_v1().controls, SOURCES);
    metrics.insert("candidate_scans_and_handoffs_ns", start.elapsed().as_nanos());

    for phase in ["candidate_fee_trial_one_ns", "candidate_fee_trial_two_ns"] {
        let start = Instant::now();
        let mut scratch = TokenizerRegistrationFeeWorkingStateV1::new(parent, &manifest.params, &action.at, extras).unwrap();
        scratch.begin_body_v1(&manifest.params, extras).unwrap();
        for (_, carrier) in &authoritative.tokenizer_registration_carriers_v1 {
            let (fee, refusal) =
                memo.scope_v1(|| scratch.apply_v1(parent, action.parent_root, &manifest.params, &action.at, carrier, extras)).unwrap();
            assert_eq!(refusal, None);
            assert_eq!(fee.carrier, carrier.carrier_v1());
            assert_eq!(fee.context.parent_root, action.parent_root);
            assert_eq!(fee.context.at, action.at);
            assert_eq!(fee.context.accepted_ordinal, carrier.accepted_ordinal_v1());
            assert_eq!(
                (fee.actual_fee, fee.registration_burn, fee.collectible_fee, fee.charge_id),
                (action.paid, action.burn, action.collectible, action.charge_id)
            );
            if let Some(prior) = authoritative.tokenizer_registration_expected_fees_v1.insert(fee.carrier, fee) {
                assert_eq!(phase, "candidate_fee_trial_two_ns");
                assert_eq!(prior, fee);
            }
            // The next one-carrier body starts from the identical uncommitted
            // parent view. Count rollback after the last copy as well.
            scratch.begin_body_v1(&manifest.params, extras).unwrap();
        }
        metrics.insert(phase, start.elapsed().as_nanos());
    }
    assert_eq!(authoritative.tokenizer_registration_expected_fees_v1.len(), SOURCES as usize);

    let mut observed_root = None;
    for phase in ["candidate_observed_one_ns", "candidate_observed_two_ns"] {
        let start = Instant::now();
        let mut folded =
            palw_v2_pre_object_base_retained_v1(parent, &manifest.params, &action.at, false, false, false, false, extras).unwrap();
        let mut first_root = None;
        for (_, carrier) in &authoritative.tokenizer_registration_carriers_v1 {
            let (next, fee, refusal) = memo
                .scope_v1(|| {
                    palw_v2_apply_one_tokenizer_registration_owned_v1(
                        folded,
                        parent,
                        action.parent_root,
                        &manifest.params,
                        &action.at,
                        carrier,
                        false,
                        false,
                        false,
                        false,
                        extras,
                    )
                })
                .unwrap();
            assert_eq!(refusal, None);
            assert_eq!(authoritative.tokenizer_registration_expected_fees_v1.get(&fee.carrier), Some(&fee));
            if first_root.is_none() {
                let root = next.state_root();
                assert_ne!(root, parent.state_root());
                first_root = Some(root);
            }
            folded = next;
        }
        let root = folded.state_root();
        assert_eq!(first_root, Some(root));
        if let Some(prior) = observed_root.replace(root) {
            assert_eq!(root, prior);
        }
        metrics.insert(phase, start.elapsed().as_nanos());
    }
    authoritative
}
