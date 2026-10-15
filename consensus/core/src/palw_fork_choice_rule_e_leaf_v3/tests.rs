//! RFC-0018 GAP-R4 tests: the rule-E leaf v3 commits rule E v2 capital and Gate C certification state; an anchor verifies with it;
//! a tampered capital or certification state is rejected; below the v3 fences every encoding is unchanged. **Unverified source
//! (2026-10-15): written, not built and not run.**

use super::*;
use crate::config::params::palw_t12_shipped_params;
use crate::palw_fork_choice_commitment_v1::{palw_committed_state_root_v1, palw_fork_choice_envelope_root_v1};
use crate::palw_fork_choice_rule_e_v1::PalwRuleECertificationV1;
use crate::palw_state_v2::{
    PalwBlockContextV2, PalwBondStateV2, PalwBondStatusV2, PalwClaimPhaseV2, palw_claim_template_v1,
};
use crate::tx::TransactionOutpoint;

const POINT_DAA: u64 = 5_000;
const POINT_BLUE: u64 = 4_000;

fn bond_key(i: u32) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(0x3E), i))
}

fn bond(collateral: u64) -> PalwBondStateV2 {
    PalwBondStateV2 {
        pubkey: vec![1; 4],
        operator_id: Hash64::from_u64_word(collateral),
        collateral,
        slashed: 0,
        status: PalwBondStatusV2::Active,
        registered_daa: 1,
        payout_payload: Hash64::from_u64_word(9),
        capable_classes: Default::default(),
    }
}

/// testnet-12 with the commitment and rule E armed, and — where `v3` — rule E v2 too (fixtures; validation is not asked).
fn params(v3: bool) -> Params {
    let mut p = palw_t12_shipped_params();
    p.palw_fork_choice_commitment_v1 = Some(ForkActivation::new(0));
    p.palw_fork_choice_rule_e_v1 = Some(ForkActivation::new(0));
    if v3 {
        p.palw_rule_e_participation_v2 = Some(ForkActivation::new(0));
    }
    p
}

fn state() -> PalwChainStateV2 {
    let mut s = PalwChainStateV2::genesis();
    for (i, c) in [(1u32, 13_000u64), (2, 26_000), (3, 1_000_000)] {
        s.set_bond_for_tests(bond_key(i), Some(bond(c)));
    }
    for n in 0..3u64 {
        let mut c = palw_claim_template_v1(Hash64::from_u64_word(0xC1A5), bond_key(n as u32 + 1), POINT_DAA - 10, 0, 0);
        c.accepted_blue_score = POINT_BLUE - 5 + n;
        c.phase = PalwClaimPhaseV2::Final { final_daa: POINT_DAA - 2 };
        s.set_false_valid_rows_for_tests(Hash64::from_u64_word(0x1000 + n), Some(c), None, None, 0);
    }
    s.set_last_point_for_tests(Some(PalwBlockContextV2 {
        block: Hash64::from_u64_word(0xB10C),
        daa_score: POINT_DAA,
        blue_score: POINT_BLUE,
        subsidy: 0,
    }));
    s
}

fn point(s: &PalwChainStateV2) -> PalwForkChoicePointV1 {
    let p = s.last_point().unwrap();
    PalwForkChoicePointV1 { block: p.block, daa_score: p.daa_score, blue_score: p.blue_score }
}

/// Below the v3 fences the committed root is the one v1/v2 committed, byte for byte.
#[test]
fn rfc18_r4_below_the_v3_fences_every_encoding_is_unchanged() {
    let s = state();
    // No commitment fence: the flat ADR-0043 root.
    let flat = palw_t12_shipped_params();
    assert_eq!(palw_rule_e_commit_form_v1(&flat, POINT_DAA), PalwCommitFormV1::Flat);
    assert_eq!(palw_committed_state_root_v2(&s, &flat), s.state_root());
    // Commitment only: exactly the v1 helper.
    let mut v1 = palw_t12_shipped_params();
    v1.palw_fork_choice_commitment_v1 = Some(ForkActivation::new(0));
    assert_eq!(palw_rule_e_commit_form_v1(&v1, POINT_DAA), PalwCommitFormV1::V1);
    assert_eq!(palw_committed_state_root_v2(&s, &v1), palw_committed_state_root_v1(&s, v1.palw_fork_choice_commitment_v1));
    // Rule E without v2 / Gate C: the rule-E leaf v2 envelope, as the v39 anchor has always expected.
    let p2 = params(false);
    assert_eq!(palw_rule_e_commit_form_v1(&p2, POINT_DAA), PalwCommitFormV1::RuleEV2);
    let leaf2 = PalwForkChoiceLeafV2::of(&s, &p2).expect("a rule-E leaf v2");
    let mut h = blake2b_simd::Params::new().hash_length(64).key(PALW_FORK_CHOICE_DOMAIN_V1).to_state();
    h.update(&leaf2.encode());
    h.update(s.state_root().as_bytes().as_slice());
    assert_eq!(palw_committed_state_root_v2(&s, &p2).as_bytes().as_slice(), h.finalize().as_bytes());
    // The v1 envelope helper is untouched.
    let leaf1 = PalwForkChoiceLeafV1::of(&s).unwrap();
    assert_eq!(palw_committed_state_root_v2(&s, &v1), palw_fork_choice_envelope_root_v1(&leaf1, &s.state_root()));
    // Leaf v2's window leaf does not read RFC-0018's certification field (its pre-RFC-0018 bytes).
    let window = s
        .palw_rule_e_records_v1(
            p2.palw_rule_e_state_params_v1().unwrap(),
            false,
            None,
            &crate::palw_fork_choice_rule_e_v1::PalwRuleENoBondBudgetV1,
            |_, _| true,
        )
        .unwrap();
    let mut flipped = window[0];
    flipped.certification = PalwRuleECertificationV1::Uncertified;
    assert_eq!(
        crate::palw_fork_choice_rule_e_leaf_v2::record_leaf(&window[0]),
        crate::palw_fork_choice_rule_e_leaf_v2::record_leaf(&flipped)
    );
    assert_ne!(record_leaf_v3(&window[0]), record_leaf_v3(&flipped), "leaf v3 commits it");
}

/// An anchor verifies with the v3 leaf; node and reader agree on the form and the capital total.
#[test]
fn rfc18_r4_an_anchor_verifies_with_the_v3_leaf() {
    let (s, p) = (state(), params(true));
    assert_eq!(palw_rule_e_commit_form_v1(&p, POINT_DAA), PalwCommitFormV1::RuleEV3);
    let committed = palw_committed_state_root_v2(&s, &p);
    let leaf = palw_committed_leaf_bytes_v2(&s, &p).expect("a v3 leaf");
    assert_eq!(leaf.len(), PALW_FORK_CHOICE_LEAF_V3_LEN);
    let view = palw_verify_committed_leaf_v2(&p, &committed, &point(&s), &leaf, &s.state_root()).expect("the anchor verifies");
    assert_eq!(view.form, PalwCommitFormV1::RuleEV3);
    assert_eq!(view.registry_capital_total, Some(13_000 + 26_000 + 1_000_000));
    // Registry and window openings verify against the leaf, the capital and the certification state come back.
    let decoded = PalwForkChoiceLeafV3::decode(&leaf).unwrap();
    let entries = palw_rule_e_registry_entries_v3(&s);
    let opening = palw_rule_e_open_registry_v3(&entries, &bond_key(2));
    assert_eq!(palw_rule_e_verify_registry_v3(&decoded, &bond_key(2), &opening), Ok(Some(26_000)));
    let window = prove_window_suffix_v3(&s, &p, POINT_BLUE - 10).expect("a v3 window opening");
    let records = verify_window_suffix_v3(&decoded, POINT_BLUE - 10, &window).expect("the window verifies");
    assert_eq!(records.len(), 3);
    // The client's pair is the node's accumulator with capital (Gate C not in force here: NotGoverned).
    let pair = palw_rule_e_pair_from_openings_v3(&records, &[], |k| entries.iter().find(|(b, _)| b == k).map(|(_, c)| *c), &decoded)
        .unwrap();
    assert_eq!(pair.a.participation_capital_v2, 13_000 + 26_000 + 1_000_000);
    assert_eq!(pair.even_split_min_capital_v2, (13_000u128 + 26_000 + 1_000_000).div_ceil(3));
}

/// A tampered capital or certification state is rejected.
#[test]
fn rfc18_r4_a_tampered_capital_or_certification_state_is_rejected() {
    let (s, p) = (state(), params(true));
    let committed = palw_committed_state_root_v2(&s, &p);
    let leaf = palw_committed_leaf_bytes_v2(&s, &p).unwrap();
    let decoded = PalwForkChoiceLeafV3::decode(&leaf).unwrap();
    // (1) the leaf's capital total
    let mut forged = decoded;
    forged.registry_capital_total += 1;
    assert_eq!(
        palw_verify_committed_leaf_v2(&p, &committed, &point(&s), &forged.encode(), &s.state_root()),
        Err(PalwCommittedLeafErrorV2::RootMismatch)
    );
    // (2) one bond's capital in a registry opening
    let entries = palw_rule_e_registry_entries_v3(&s);
    let mut opening = palw_rule_e_open_registry_v3(&entries, &bond_key(1));
    opening.entries[0].1 = 26_000;
    assert_eq!(palw_rule_e_verify_registry_v3(&decoded, &bond_key(1), &opening), Err(PalwRuleELeafV2ErrorV1::RootMismatch));
    // (3) one record's certification state in a window opening
    let mut window = prove_window_suffix_v3(&s, &p, POINT_BLUE - 10).unwrap();
    window.records[1].certification = PalwRuleECertificationV1::Certified;
    assert_eq!(verify_window_suffix_v3(&decoded, POINT_BLUE - 10, &window), Err(PalwRuleELeafV2ErrorV1::RootMismatch));
    // (4) a v2 leaf offered where the reader's ruleset requires v3 (a downgrade) is refused by grammar
    let v2_leaf = PalwForkChoiceLeafV2::of(&s, &params(false)).unwrap().encode();
    assert_eq!(
        palw_verify_committed_leaf_v2(&p, &committed, &point(&s), &v2_leaf, &s.state_root()),
        Err(PalwCommittedLeafErrorV2::BadLeaf)
    );
    // (5) another point
    let mut other = point(&s);
    other.blue_score += 1;
    assert_eq!(
        palw_verify_committed_leaf_v2(&p, &committed, &other, &leaf, &s.state_root()),
        Err(PalwCommittedLeafErrorV2::PointMismatch)
    );
}
