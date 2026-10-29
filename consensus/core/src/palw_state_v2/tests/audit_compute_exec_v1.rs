//! **Compute-claim audit, lane compute-exec (2026-10-22): a decoded answer is paid again.**
//!
//! A deterministic causal model makes every position of a run a function of the tokens at and before it. A free-prompt claim's
//! run touches `prompt + decode − 1` positions; the chain pays for the positions past the longest prefix of the PROMPT any claim
//! of the class already committed (ADR-0145 §6, `fp_claimed_prompt_ids_of`) — and it records only prompts. The positions a paid
//! claim DECODED are recorded nowhere, so:
//!
//! * **CX-01 (continuation re-sale)** — claim C0 = (P, decode D) answers `g`. Claim C1 = (P ‖ g, decode d) touches C0's
//!   positions again as "prefill" and is credited for all of them, though the producer holds them in its KV cache from C0
//!   (no forward pass). Stepping the prompt one answer token at a time re-sells C0's decode positions on every rung.
//! * **CX-02 (cross-bond copy)** — the same prompt under a second bond (`fp_work_id_v1` is per bond) is credited its whole
//!   decode again; the leaves are bond-independent values the producer re-hashes under the second bond's context.
//!
//! Both claims are HONEST executions — every seat's replay reproduces them, no court can convict them — so the only defence is
//! the price. These tests state the property the price must have: a claim is credited for no position a paid claim of the
//! class already computed with the same tokens. The fix is `palw_fp_answer_accounting_v1` (dormant): past it a commitment carries
//! its answer and the chain records `prompt ‖ answer` (`crate::palw_fp_answer_accounting_v1`). Below it every number is as before,
//! which `audit_cx_below_the_fence_the_price_is_unchanged` holds.
use super::*;

fn leaves(prompt: u32, decode: u32) -> u64 {
    crate::palw_step::step_leaf_count_of_tokens_capped_v1(&derived_profile(), prompt, decode, 1 << 26).unwrap()
}

fn prefill(positions: u32) -> u64 {
    crate::palw_step::prefill_leaf_count_of_tokens_capped_v1(&derived_profile(), positions, 1 << 26).unwrap()
}

/// The leaves-era price of `credited` leaves on the fixture (quantum 1,000 leaves, cap `fp_max_quanta_per_receipt`).
fn priced(p: &PalwStateParamsV2, credited: u64) -> u64 {
    (credited / 1_000).min(p.fp_max_quanta_per_receipt as u64) * 1_000
}

/// The chain the reproducers fold on: the floor's class, its free-prompt lane certified, `palw_fp_derived_work` armed —
/// testnet-12's live accounting (derived work at genesis; the decode rules dormant).
fn published() -> (PalwStateParamsV2, PalwChainStateV2, Hash64, PalwTransitionExtrasV1) {
    let (p, base, class, _) = derived_work_chain();
    let armed = derived_work_armed();
    let (published, _) = apply_derived(&base, &p, &ctx(3, 102, 3), &[lane_certification(class)], &armed).expect("published");
    (p, published, class, armed)
}

/// [`published`]'s extras with `palw_fp_answer_accounting_v1` in force and `answers` carried by native ingress.
fn answered(armed: &PalwTransitionExtrasV1, answers: &[(u64, &[u32])]) -> PalwTransitionExtrasV1 {
    PalwTransitionExtrasV1 {
        fp_answer_accounting_active: true,
        fp_claim_answers: answers.iter().map(|(word, ids)| (h64(*word), ids.to_vec())).collect(),
        ..armed.clone()
    }
}

/// **CX-01**: C1's prompt is C0's prompt followed by C0's own answer. C0 already computed positions `0 ..= 8 + 24 − 2` (30); C1
/// touches `0 ..= 32 + 2 − 2` (32). Only positions 31 and 32 are new work.
#[test]
fn audit_cx01_a_paid_answer_resold_as_the_next_prompt_is_credited_again() {
    let (p, chain, _class, armed) = published();
    let prompt: Vec<u32> = (0..8).collect();
    let answer: Vec<u32> = (500..524).collect();
    let (d0, d1) = (24u32, 2u32);
    let continued: Vec<u32> = prompt.iter().chain(answer.iter()).copied().collect();
    let c1_answer = [900u32, 901];
    let fenced = answered(&armed, &[(0xC0, &answer), (0xC1, &c1_answer)]);
    let (c0_state, _) =
        apply_derived(&chain, &p, &ctx(4, 103, 4), &[derived_commit(0xC0, &prompt, d0, leaves(8, d0))], &fenced).expect("C0 applies");
    let (c1_state, _) = apply_derived(&c0_state, &p, &ctx(5, 104, 5), &[derived_commit(0xC1, &continued, d1, leaves(32, d1))], &fenced)
        .expect("C1 applies");
    let c1 = c1_state.claim(&h64(0xC1)).unwrap();
    // C0 touched positions 0 ..= 30 (31 positions); what is new in C1 is everything past them.
    let new_work = leaves(32, d1) - prefill(31);
    let resold = leaves(32, d1) - prefill(8);
    assert!(
        c1.pwu <= priced(&p, new_work),
        "CX-01: C1 is credited {} pwu; the positions C0 had not computed are worth {} (C0's answer re-sold as prefill: {})",
        c1.pwu,
        priced(&p, new_work),
        priced(&p, resold)
    );
    // The record keeps the run's own count: the RUN is the same run, what changes is what it is credited for.
    assert_eq!(c1.work_leaves, leaves(32, d1));
    // Every rung of the ladder — the prompt stepped one answer token at a time — is refused or credited only its new tail.
    let rung: Vec<u32> = prompt.iter().chain(answer[..4].iter()).copied().collect();
    let rung_answer = answer[4..].to_vec();
    let laddered = answered(&armed, &[(0xC0, &answer), (0xC2, &rung_answer)]);
    let (c0_again, _) =
        apply_derived(&chain, &p, &ctx(4, 103, 4), &[derived_commit(0xC0, &prompt, d0, leaves(8, d0))], &laddered).expect("C0");
    assert_eq!(
        apply_derived(&c0_again, &p, &ctx(5, 104, 5), &[derived_commit(0xC2, &rung, 20, leaves(12, 20))], &laddered).unwrap_err(),
        PalwStateV2Error::ZeroQuanta,
        "a rung whose every position C0 computed is not taken"
    );
}

/// **CX-02**: the same prompt, the same greedy answer, a second bond of the same operator. Nothing in the second run is new.
#[test]
fn audit_cx02_the_same_run_under_a_second_bond_is_credited_again() {
    let (p, chain, _class, armed) = published();
    let prompt: Vec<u32> = (0..8).collect();
    let answer: Vec<u32> = (500..524).collect();
    let d = 24u32;
    let fenced = answered(&armed, &[(0xD1, &answer), (0xD2, &answer)]);
    let (first, _) =
        apply_derived(&chain, &p, &ctx(4, 103, 4), &[derived_commit_from(1, 0xD1, &prompt, d, leaves(8, d))], &fenced).expect("bond 1");
    let copy = apply_derived(&first, &p, &ctx(5, 104, 5), &[derived_commit_from(2, 0xD2, &prompt, d, leaves(8, d))], &fenced);
    let credited = copy.as_ref().ok().and_then(|(state, _)| state.claim(&h64(0xD2))).map_or(0, |claim| claim.pwu);
    assert_eq!(credited, 0, "CX-02: a copy of a paid run under a second bond is credited {credited} pwu for no new position");
    assert_eq!(copy.unwrap_err(), PalwStateV2Error::ZeroQuanta);
}

/// The honest side of the rule: a second turn whose prompt is the first turn's prompt, its answer and a NEW user message is
/// credited the new message and its own answer; a run that shares the prompt but decodes a different answer (the producer's
/// sampler, another model state) is credited from the first differing position on.
#[test]
fn audit_cx_an_honest_next_turn_is_credited_its_new_positions() {
    let (p, chain, _class, armed) = published();
    let prompt: Vec<u32> = (0..8).collect();
    let answer: Vec<u32> = (500..524).collect();
    let turn: Vec<u32> = prompt.iter().chain(answer.iter()).chain([77u32, 78, 79, 80].iter()).copied().collect();
    let turn_answer = [600u32, 601];
    let fenced = answered(&armed, &[(0xE0, &answer), (0xE1, &turn_answer)]);
    let (first, _) =
        apply_derived(&chain, &p, &ctx(4, 103, 4), &[derived_commit(0xE0, &prompt, 24, leaves(8, 24))], &fenced).expect("turn 1");
    let (second, _) =
        apply_derived(&first, &p, &ctx(5, 104, 5), &[derived_commit(0xE1, &turn, 2, leaves(36, 2))], &fenced).expect("turn 2");
    // Turn 1 computed positions 0 ..= 30; turn 2 touches 0 ..= 36, so 31 ..= 36 are new (the last answer token's own position,
    // the four new message tokens and the decode).
    assert_eq!(second.claim(&h64(0xE1)).unwrap().pwu, priced(&p, leaves(36, 2) - prefill(31)));
}

/// A commitment past the fence that carries no answer, or a short one, is not taken; below the fence nothing is asked.
#[test]
fn audit_cx_past_the_fence_a_commitment_without_its_answer_is_refused() {
    let (p, chain, _class, armed) = published();
    let prompt: Vec<u32> = (0..8).collect();
    let none = answered(&armed, &[]);
    assert_eq!(
        apply_derived(&chain, &p, &ctx(4, 103, 4), &[derived_commit(0xF0, &prompt, 24, leaves(8, 24))], &none).unwrap_err(),
        PalwStateV2Error::FreePromptAnswerNotAccountable { claim: h64(0xF0), carried: None, decoded: 24 }
    );
    let short = answered(&armed, &[(0xF0, &[1, 2, 3])]);
    assert_eq!(
        apply_derived(&chain, &p, &ctx(4, 103, 4), &[derived_commit(0xF0, &prompt, 24, leaves(8, 24))], &short).unwrap_err(),
        PalwStateV2Error::FreePromptAnswerNotAccountable { claim: h64(0xF0), carried: Some(3), decoded: 24 }
    );
}

/// **Dormant**: below the fence the same two claims are priced exactly as the live fold prices them (the defect stands there by
/// design — the fence is the change), no answer row is written, and an answer carried anyway changes nothing.
#[test]
fn audit_cx_below_the_fence_the_price_is_unchanged() {
    let (p, chain, class, armed) = published();
    let prompt: Vec<u32> = (0..8).collect();
    let answer: Vec<u32> = (500..524).collect();
    let continued: Vec<u32> = prompt.iter().chain(answer.iter()).copied().collect();
    let carried_but_dormant = PalwTransitionExtrasV1 { fp_claim_answers: [(h64(0xC0), answer.clone())].into(), ..armed.clone() };
    for extras in [&armed, &carried_but_dormant] {
        let (c0, _) = apply_derived(&chain, &p, &ctx(4, 103, 4), &[derived_commit(0xC0, &prompt, 24, leaves(8, 24))], extras).unwrap();
        assert_eq!(c0.fp_paid_sequences_of_v1(&class, 0).len(), 1, "no answer row below the fence");
        let (c1, _) = apply_derived(&c0, &p, &ctx(5, 104, 5), &[derived_commit(0xC1, &continued, 2, leaves(32, 2))], extras).unwrap();
        assert_eq!(c1.claim(&h64(0xC1)).unwrap().pwu, priced(&p, leaves(32, 2) - prefill(8)), "the live price, unchanged");
    }
}

/// **CX-01, the live testnet-12 form** (`palw_fp_decode_rules` armed with the int-11 flag day at DAA 5,300 on the int-12 release):
/// the prompt is priced at zero and the decode is credited, so the ladder re-sells C0's DECODE positions. C2 = (P ‖ g[..4],
/// decode 20) emits exactly g[4..24] — every one of its positions is one C0 computed. Past the fence it is not taken; a rung that
/// decodes past C0's last computed position is credited that tail only.
#[test]
fn audit_cx01_live_under_the_decode_rules_a_rung_of_a_paid_answer_is_credited_again() {
    let (p, chain, _class, armed) = published();
    let p = p.with_fp_decode_rules_v1(Some(0));
    let prompt: Vec<u32> = (0..8).collect();
    let answer: Vec<u32> = (500..524).collect();
    let rung: Vec<u32> = prompt.iter().chain(answer[..4].iter()).copied().collect();
    let longer: Vec<u32> = answer[4..].iter().copied().chain(700..704).collect();
    let fenced = answered(&armed, &[(0xC0, &answer), (0xC2, &answer[4..]), (0xC3, &longer)]);
    let (c0_state, _) =
        apply_derived(&chain, &p, &ctx(4, 103, 4), &[derived_commit(0xC0, &prompt, 24, leaves(8, 24))], &fenced).expect("C0 applies");
    let resold = apply_derived(&c0_state, &p, &ctx(5, 104, 5), &[derived_commit(0xC2, &rung, 20, leaves(12, 20))], &fenced);
    let credited = resold.as_ref().ok().and_then(|(state, _)| state.claim(&h64(0xC2))).map_or(0, |claim| claim.pwu);
    assert_eq!(credited, 0, "CX-01 (decode rules): a rung whose every position C0 computed is credited {credited} pwu");
    // Four tokens past C0's answer: positions 31 ..= 34 are new (C0 computed 0 ..= 30), and only they are credited.
    let (tail, _) = apply_derived(&c0_state, &p, &ctx(5, 104, 5), &[derived_commit(0xC3, &rung, 24, leaves(12, 24))], &fenced)
        .expect("a rung with a new tail applies");
    assert_eq!(tail.claim(&h64(0xC3)).unwrap().pwu, priced(&p, leaves(31, 5) - prefill(31)));
}

/// **RFC-0038:** the answer fold records one settled carrier. A second transaction id for the
/// same claim is not what the index stores, so a later reader does not choose between them.
#[test]
fn rfc0038_answer_fold_indexes_the_settled_carrier_and_not_another_same_claim_tx() {
    use crate::palw_fp_answer_accounting_v1::PALW_FP_WINNING_CARRIER_TABLE_V1;
    let (p, mut chain, _class, armed) = published();
    chain.kernel_route = Some(crate::palw_kernel_route_v1::PalwKernelRouteStateV1::new(
        crate::palw_kernel_route_v1::palw_kernel_route_policy_v1(h64(1), h64(2)),
        None,
        misaka_palw_kernel::rows::LedgerScalarsV1 { daa: 1, burned: 0 },
    ));
    let prompt: Vec<u32> = (0..8).collect();
    let answer: Vec<u32> = (500..524).collect();
    let settled = h64(0x5E77);
    let other = h64(0x07EE);
    let mut fenced = answered(&armed, &[(0xC0, &answer)]);
    fenced.fp_winning_carriers.insert(h64(0xC0), settled);
    let (state, _) =
        apply_derived(&chain, &p, &ctx(4, 103, 4), &[derived_commit(0xC0, &prompt, 24, leaves(8, 24))], &fenced).expect("C0 applies");
    let raw = state
        .kernel_route
        .as_ref()
        .unwrap()
        .aux
        .get(&(PALW_FP_WINNING_CARRIER_TABLE_V1, h64(0xC0).as_byte_slice().to_vec()))
        .expect("the settled carrier is indexed");
    let stored = crate::Hash64::try_from(raw.as_slice()).expect("the row is the carrier id");
    assert_eq!(stored, settled);
    assert_ne!(stored, other);
}
