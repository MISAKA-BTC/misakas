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
//! class already computed with the same tokens.
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

/// **CX-01**: C1's prompt is C0's prompt followed by C0's own answer. C0 already computed positions `0 ..= 8 + 24 − 2` (30); C1
/// touches `0 ..= 32 + 2 − 2` (32). Only positions 31 and 32 are new work.
#[test]
fn audit_cx01_a_paid_answer_resold_as_the_next_prompt_is_credited_again() {
    let (p, chain, _class, armed) = published();
    let prompt: Vec<u32> = (0..8).collect();
    let answer: Vec<u32> = (500..524).collect();
    let (d0, d1) = (24u32, 2u32);
    let (c0_state, _) =
        apply_derived(&chain, &p, &ctx(4, 103, 4), &[derived_commit(0xC0, &prompt, d0, leaves(8, d0))], &armed).expect("C0 applies");
    let continued: Vec<u32> = prompt.iter().chain(answer.iter()).copied().collect();
    let (c1_state, _) = apply_derived(&c0_state, &p, &ctx(5, 104, 5), &[derived_commit(0xC1, &continued, d1, leaves(32, d1))], &armed)
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
}

/// **CX-02**: the same prompt, the same greedy answer, a second bond of the same operator. Nothing in the second run is new.
#[test]
fn audit_cx02_the_same_run_under_a_second_bond_is_credited_again() {
    let (p, chain, _class, armed) = published();
    let prompt: Vec<u32> = (0..8).collect();
    let d = 24u32;
    let (first, _) =
        apply_derived(&chain, &p, &ctx(4, 103, 4), &[derived_commit_from(1, 0xD1, &prompt, d, leaves(8, d))], &armed).expect("bond 1");
    let copy = apply_derived(&first, &p, &ctx(5, 104, 5), &[derived_commit_from(2, 0xD2, &prompt, d, leaves(8, d))], &armed);
    let credited = copy.as_ref().ok().and_then(|(state, _)| state.claim(&h64(0xD2))).map_or(0, |claim| claim.pwu);
    assert_eq!(credited, 0, "CX-02: a copy of a paid run under a second bond is credited {credited} pwu for no new position");
}

/// **CX-01, the live testnet-12 form** (`palw_fp_decode_rules` armed with the int-11 flag day at DAA 5,300 on the int-12 release):
/// the prompt is priced at zero and the decode is credited, so the ladder re-sells C0's DECODE positions. C2 = (P ‖ g[..4],
/// decode 20) emits exactly g[4..24] — every one of its positions is one C0 computed.
#[test]
fn audit_cx01_live_under_the_decode_rules_a_rung_of_a_paid_answer_is_credited_again() {
    let (p, chain, _class, armed) = published();
    let p = p.with_fp_decode_rules_v1(Some(0));
    let prompt: Vec<u32> = (0..8).collect();
    let answer: Vec<u32> = (500..524).collect();
    let (c0_state, _) =
        apply_derived(&chain, &p, &ctx(4, 103, 4), &[derived_commit(0xC0, &prompt, 24, leaves(8, 24))], &armed).expect("C0 applies");
    let rung: Vec<u32> = prompt.iter().chain(answer[..4].iter()).copied().collect();
    let resold = apply_derived(&c0_state, &p, &ctx(5, 104, 5), &[derived_commit(0xC2, &rung, 20, leaves(12, 20))], &armed);
    let credited = resold.as_ref().ok().and_then(|(state, _)| state.claim(&h64(0xC2))).map_or(0, |claim| claim.pwu);
    assert_eq!(credited, 0, "CX-01 (decode rules): a rung whose every position C0 computed is credited {credited} pwu");
}
