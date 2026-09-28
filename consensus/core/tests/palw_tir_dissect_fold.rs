//! **RFC-0002 Phase F, step F7 — the exit test: the IR history dissection played on the chain.**
//!
//! The dense GQA corpus model is run as a 24-position job (two-position history tiles, so the
//! dissected leaf with the widest history covers twelve tiles: four rounds at arity 2), registered
//! as an IR class and claimed. Every block goes through the transition and is checked three ways —
//! the delta re-applies and reverts, and the child's carriage (with the `0xC1` tail while a phase is
//! open) reloads under its committed root — and every move is first put through the acceptance
//! layer's checks as the processor calls them (the signatures, the arity, the root claim's
//! finalize at the IR court's limits, the bottom's adjudication).
//!
//! * **Honest:** the court narrows to the widest dissected leaf, the responder's root claim opens
//!   the IR phase, every round folds, the challenger names children down to one tile, and the
//!   bottom adjudicates `ChallengerDefeated`: the executor is acquitted, the claim stands.
//! * **A lie in each reduction:** the executor committed the tile a false total of that reduction
//!   finalizes to (or the honest tile, when the lie is absorbed), keeps every round folding by
//!   pushing the lie into a child, and is convicted at the bottom (`CourtFraud`); a round that
//!   cannot fold is refused at the fold, and the responder's silence then loses.
//! * **Silence loses:** the responder's at the root claim and at a round, the challenger's at a
//!   choice; at the bottom the burden is the challenger's and the backstop ends it on its side.
//! * **The held regime:** a one-move accusation naming the dissected leaf opens the same phase at
//!   that leaf (`CourtOpened` is refused there), and the dissection convicts from there.
//! * **Every move is refused out of turn, by the wrong party, and below the fence.**

#[path = "palw_tir_fixture_common.rs"]
mod fixture;
use fixture::*;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_attempt_v2::{
    PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, attempt_id_v2, challenge_v2,
};
use kaspa_consensus_core::palw_bisect::{
    PALW_BISECT_OBJECT_VERSION_V1, PalwBisectDisclosureV1, PalwBisectSpaceV1, PalwBisectTurnV1, PalwBisectVerdictV1,
};
use kaspa_consensus_core::palw_court_v2::{
    PALW_COURT_V2_MLDSA87_ATTN_CHALLENGER_CONTEXT, PALW_COURT_V2_MLDSA87_ATTN_RESPONDER_CONTEXT, PalwCourtV2Error,
    PalwCourtVerdictProofV2, adjudicate_court_close_v3, check_court_tir_choice_acceptance_v1,
    check_court_tir_root_claim_acceptance_v1, check_court_tir_root_claim_admits_v1, check_court_tir_round_acceptance_v1,
    court_session_id_v2, palw_tir_dissection_move_is_admissible_v1,
};
use kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2;
use kaspa_consensus_core::palw_panel_v2::{PalwReceiptVerdictV2, PalwSeatReceiptV2};
use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwConsensusObjectV2, PalwCourtVerdictV2, PalwPanelSeatV2,
    PalwPwuRuleV2, PalwStateCarriageV2, PalwStateDeltaV2, PalwStateParamsV2, PalwStateV2Error, PalwTransitionExtrasV1,
    PalwVoidReasonV2, apply_delta_v2, apply_palw_transition_v2_with_extras, palw_court_move_spends_the_slot_v1,
    palw_object_is_tir_dissection_move_v1, palw_object_is_tir_v1, palw_operator_id_v2, revert_delta_v2,
};
use kaspa_consensus_core::palw_tir_class_v1::PalwTirAdmissionCarriageV1;
use kaspa_consensus_core::palw_tir_court_v1::{
    build_tir_dissect_bottom_v1, build_tir_dissect_round_v1, build_tir_named_leaf_refutation_v1, build_tir_root_claim_v1,
    tir_root_claim_finalizes_to_v1,
};
use kaspa_consensus_core::palw_tir_dissect_v1::{
    PALW_TIR_DISSECT_OBJECT_VERSION_V1, PalwTirDissectChoiceV1, PalwTirDissectPhaseV1, PalwTirDissectRoundV1, PalwTirDissectSiteV1,
    PalwTirFoldV1, PalwTirRootClaimV1, palw_tir_choice_message_v1, palw_tir_dissect_site_v1, palw_tir_root_claim_message_v1,
    palw_tir_round_message_v1,
};
use kaspa_consensus_core::palw_tir_one_move_v1::{
    PalwTirOneMoveOutcomeV1, palw_tir_one_move_accusation_v1, palw_tir_one_move_outcome_v1,
};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

const TURN: u64 = 20;
const WINDOW_COURT: u64 = 500;
const LADDER: u64 = 1 << 26;
const PRODUCER: u64 = 1;
const CHALLENGER: u64 = 2;

fn h64(v: u64) -> Hash64 {
    Hash64::from_u64_word(v)
}

fn bond_key(n: u64) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(n), index: 0 })
}

fn pubkey(n: u64) -> Vec<u8> {
    vec![6 + n as u8; 4]
}

fn op_key(n: u64) -> Vec<u8> {
    vec![20 + n as u8; 8]
}

/// The tests' signature: the key, the context and the message, verbatim — exact about who signed
/// what under which context, which is what the acceptance checks decide (the ML-DSA-87 arithmetic
/// is the processor's verifier's, tested where it lives).
fn sign(key: &[u8], message: &[u8], context: &[u8]) -> Vec<u8> {
    [key, context, message].concat()
}

fn verify(key: &[u8], message: &[u8], signature: &[u8], context: &[u8]) -> bool {
    signature == sign(key, message, context).as_slice()
}

fn params() -> PalwStateParamsV2 {
    PalwStateParamsV2::new(100, 10, 10, 20, WINDOW_COURT, 1000, h64(1), 4, 1000, 100, 1000, 0)
        .unwrap()
        .with_fp_quanta(8, 64)
        .unwrap()
        .with_tir_from_daa(Some(0))
        .with_turn_deadline_daa(TURN)
        .unwrap()
}

/// The ruleset's court the acceptance layer judges at (its default ceilings).
fn court() -> PalwCourtParamsV2 {
    PalwCourtParamsV2::new(LADDER, TURN, 2).expect("a court")
}

// =================================================================================================
// The world: the class, its honest run, and the dissected leaf with the widest history
// =================================================================================================

struct World {
    f: Fixture,
    honest: Execution,
    leaf: u64,
    site: PalwTirDissectSiteV1,
}

fn world() -> World {
    let (name, program, params, tokens) =
        programs().into_iter().find(|(n, ..)| n == "dense-gqa-2layer").expect("the dense GQA corpus model");
    let f = fixture_with(name, program, params, tokens, 16, 9);
    let honest = f.honest();
    let intervals = f.intervals.as_ref().expect("admissible");
    let (leaf, site) = f
        .leaves
        .iter()
        .enumerate()
        .filter_map(|(i, leaf)| palw_tir_dissect_site_v1(&f.space, intervals, leaf).map(|s| (i as u64, s)))
        .max_by_key(|(i, s)| (s.history_positions, s.reductions.len(), std::cmp::Reverse(*i)))
        .expect("the dense model has dissected leaves");
    assert_eq!(site.history_positions, 24, "the widest history: every position of the job");
    assert!(site.reductions.len() >= 3, "a softmax attention: its maximum, its exponent sum, its value contraction");
    World { f, honest, leaf, site }
}

// =================================================================================================
// The chain
// =================================================================================================

struct Run {
    p: PalwStateParamsV2,
    s: PalwChainStateV2,
    daa: u64,
    extras: PalwTransitionExtrasV1,
}

impl Run {
    fn new() -> Self {
        Self { p: params(), s: PalwChainStateV2::genesis(), daa: 0, extras: PalwTransitionExtrasV1::default() }
    }

    fn ctx(daa: u64) -> PalwBlockContextV2 {
        PalwBlockContextV2 { block: h64(0xB10C_0000 + daa), daa_score: daa, blue_score: daa, subsidy: 0 }
    }

    fn try_at(
        &self,
        daa: u64,
        objects: &[PalwConsensusObjectV2],
        att: Option<&PalwAttemptEnvelopeV2>,
    ) -> Result<(PalwChainStateV2, PalwStateDeltaV2), PalwStateV2Error> {
        apply_palw_transition_v2_with_extras(&self.s, &self.p, &Self::ctx(daa), objects, att, false, false, false, true, &self.extras)
    }

    /// One block at `daa`: folded, re-applied, reverted, reloaded.
    fn at(&mut self, daa: u64, objects: &[PalwConsensusObjectV2], att: Option<&PalwAttemptEnvelopeV2>) {
        assert!(daa > self.daa, "DAA moves forward");
        let (child, delta) = self.try_at(daa, objects, att).unwrap_or_else(|e| panic!("the block at DAA {daa} folds: {e}"));
        child.assert_internal_consistency(&self.p).expect("internal consistency");
        child.assert_deadline_consistency(&self.p).expect("deadline consistency");
        assert_eq!(apply_delta_v2(&self.s, &delta, &self.p).expect("re-applies"), child, "DAA {daa}: the delta is the transition");
        assert_eq!(revert_delta_v2(&child, &delta, &self.p).expect("reverts"), self.s, "DAA {daa}: the delta reverts");
        let reloaded = PalwStateCarriageV2::from_state(&child)
            .into_state(&self.p, Some(child.state_root()))
            .unwrap_or_else(|e| panic!("DAA {daa}: the carriage reloads: {e}"));
        assert_eq!(reloaded, child, "DAA {daa}: reload is the state");
        self.s = child;
        self.daa = daa;
    }

    fn step(&mut self, objects: &[PalwConsensusObjectV2]) {
        self.at(self.daa + 1, objects, None);
    }

    fn refused(&self, objects: &[PalwConsensusObjectV2]) -> PalwStateV2Error {
        self.try_at(self.daa + 1, objects, None).expect_err("the fold refuses it")
    }

    fn phase(&self, sid: &Hash64) -> PalwTirDissectPhaseV1 {
        self.s.tir_dissection_v1(sid).expect("an open IR phase").clone()
    }

    fn collateral(&self, n: u64) -> u64 {
        self.s.bond(&bond_key(n)).expect("the bond").collateral
    }
}

fn bond(n: u64) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::BondRegistered {
        bond: bond_key(n),
        pubkey: pubkey(n),
        operator_pubkey: op_key(n),
        collateral: 1_000,
        payout_payload: Hash64::from_u64_word(0x9A00 + n),
        capable_classes: Default::default(),
        signature: Vec::new(),
    }
}

/// The chain up to a licensed claim of the IR class committing `x`: the floor class, the producer's
/// and the challenger's bonds and the IR class (weightless) at DAA 1, the claim at 2, its panel at 3,
/// its licence at 4.
fn licensed(w: &World, x: &Execution) -> (Run, Hash64) {
    let mut run = Run::new();
    let class_id = w.f.class_id;
    run.at(
        1,
        &[
            PalwConsensusObjectV2::ClassRegistered {
                class_id: h64(1),
                artifact_root: h64(11),
                slash_value_per_pwu: 5,
                pwu_rule: PalwPwuRuleV2::MaxPerAttempt(160),
                initial_target: u128::MAX / 2,
                share_permille: 1000,
                activation_daa: 0,
                admission: None,
            },
            bond(PRODUCER),
            bond(CHALLENGER),
            PalwConsensusObjectV2::ClassRegisteredTirV1 {
                class_id,
                artifact_root: w.f.artifact_root,
                slash_value_per_pwu: 5,
                pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: 40 },
                initial_target: u128::MAX / 2,
                share_permille: 0,
                activation_daa: 0,
                admission: Box::new(PalwTirAdmissionCarriageV1 {
                    class: w.f.class.clone(),
                    canonical: w.f.ctx.clone(),
                    registrant_bond: bond_key(PRODUCER),
                    signature: vec![9; 8],
                }),
            },
        ],
        None,
    );
    assert!(run.s.tir_class_v1(&class_id).is_some_and(|r| !r.dissected.is_empty()), "the class has dissected commit points");
    assert!(run.s.class(&class_id).expect("registered").fused_attention, "so its responder owes the court's terminal move");
    let network_domain = h64(999);
    let producer = bond_key(PRODUCER).0;
    let env = PalwAttemptEnvelopeV2 {
        attempt: PalwAttemptUnsignedV2 {
            version: PALW_ATTEMPT_V2_VERSION,
            network_domain,
            challenge: challenge_v2(network_domain, h64(5), 1_700, 1, class_id, &producer),
            class_id,
            executor_bond: producer,
            executor_pubkey: pubkey(PRODUCER),
            operator_id: palw_operator_id_v2(&op_key(PRODUCER)),
            artifact_root: w.f.artifact_root,
            trace_root: x.binding.full_logits_trace_root,
            output_root: h64(32),
            pwu: 40,
            trace_manifest_root: h64(33),
            trace_chunk_count: 1,
            trace_retention_daa: 999_999,
            execution_root: x.binding.committed_execution_root,
        },
        signature: vec![0; 8],
    };
    let claim_id = attempt_id_v2(&env.attempt);
    run.at(2, &[], Some(&env));
    let seats = vec![PalwPanelSeatV2 { bond: bond_key(PRODUCER), operator_id: palw_operator_id_v2(&op_key(PRODUCER)) }];
    run.at(3, &[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: h64(77), seats }], None);
    let receipts = vec![PalwSeatReceiptV2 {
        claim: Hash64::default(),
        verdict: PalwReceiptVerdictV2::Valid,
        seat_bond: bond_key(PRODUCER),
        signed_daa: 0,
        signature: Vec::new(),
    }];
    run.at(4, &[PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id, receipts }], None);
    assert!(matches!(run.s.claim(&claim_id).expect("live").phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));
    (run, claim_id)
}

/// A court on the claim, narrowed by real rungs to the dissected leaf (the challenger agrees exactly
/// when the leaf is at or above the midpoint).
fn court_at_leaf(run: &mut Run, claim_id: Hash64, x: &Execution, leaf: u64) -> Hash64 {
    let size = x.binding.step_leaf_count;
    let sid = court_session_id_v2(
        &claim_id,
        &x.binding.full_logits_trace_root,
        &bond_key(PRODUCER),
        &bond_key(CHALLENGER),
        PalwBisectSpaceV1::StepLeaves,
        size,
    );
    run.step(&[PalwConsensusObjectV2::CourtOpened {
        session_id: sid,
        claim: claim_id,
        challenger_bond: bond_key(CHALLENGER),
        space: PalwBisectSpaceV1::StepLeaves,
        space_size: size,
        signature: Vec::new(),
    }]);
    let mut round = 0u32;
    while run.s.court_session(&sid).expect("the session lives").ladder.terminal_index().is_none() {
        let (lo, hi) = run.s.court_session(&sid).unwrap().ladder.interval();
        let midpoint = lo + (hi - lo) / 2;
        run.step(&[PalwConsensusObjectV2::CourtDisclosed {
            session_id: sid,
            disclosure: PalwBisectDisclosureV1 {
                version: PALW_BISECT_OBJECT_VERSION_V1,
                session_id: sid,
                round,
                midpoint,
                mid_state: h64(0xD00 + round as u64),
            },
            signature: vec![0xAA; 8],
        }]);
        run.step(&[PalwConsensusObjectV2::CourtVerdictPosted {
            session_id: sid,
            verdict: PalwBisectVerdictV1 { version: PALW_BISECT_OBJECT_VERSION_V1, session_id: sid, round, agree: leaf >= midpoint },
            signature: vec![0xBB; 8],
        }]);
        round += 1;
        assert!(round < 64, "the ladder narrows");
    }
    assert_eq!(run.s.court_session(&sid).unwrap().ladder.terminal_index(), Some(leaf));
    sid
}

// =================================================================================================
// The moves, through the acceptance layer's checks
// =================================================================================================

/// The responder's root claim, signed, and admitted as the processor admits it.
fn root_claimed(run: &Run, sid: Hash64, root: PalwTirRootClaimV1, arity: u8) -> PalwConsensusObjectV2 {
    let signature =
        sign(&pubkey(PRODUCER), &palw_tir_root_claim_message_v1(&sid, &root), PALW_COURT_V2_MLDSA87_ATTN_RESPONDER_CONTEXT);
    let object = PalwConsensusObjectV2::CourtTirRootClaimed {
        session_id: sid,
        root: Box::new(root.clone()),
        arity,
        signature: signature.clone(),
    };
    assert_eq!(palw_tir_dissection_move_is_admissible_v1(&object, true), Ok(()));
    check_court_tir_root_claim_acceptance_v1(&run.s, &sid, &root, &signature, verify).expect("signed by the claim's bond");
    check_court_tir_root_claim_admits_v1(&run.s, &sid, &root, arity, arity, &court(), LADDER, PalwPromptIdsFormV1::Flat)
        .expect("the root claim finalizes to the committed tile");
    assert!(palw_court_move_spends_the_slot_v1(&run.s, &object), "the root claim's finalize takes the block's slot");
    object
}

fn dissected(run: &Run, sid: Hash64, round: PalwTirDissectRoundV1) -> PalwConsensusObjectV2 {
    let at = run.phase(&sid).round();
    let signature =
        sign(&pubkey(PRODUCER), &palw_tir_round_message_v1(&sid, at, &round), PALW_COURT_V2_MLDSA87_ATTN_RESPONDER_CONTEXT);
    check_court_tir_round_acceptance_v1(&run.s, &sid, &round, &signature, verify).expect("signed by the claim's bond at the round");
    PalwConsensusObjectV2::CourtTirDissected { session_id: sid, round, signature }
}

fn chosen(run: &Run, sid: Hash64, child: u8) -> PalwConsensusObjectV2 {
    let choice =
        PalwTirDissectChoiceV1 { version: PALW_TIR_DISSECT_OBJECT_VERSION_V1, session_id: sid, round: run.phase(&sid).round(), child };
    let signature = sign(&pubkey(CHALLENGER), &palw_tir_choice_message_v1(&choice), PALW_COURT_V2_MLDSA87_ATTN_CHALLENGER_CONTEXT);
    check_court_tir_choice_acceptance_v1(&run.s, &sid, &choice, &signature, verify).expect("signed by the session's challenger");
    PalwConsensusObjectV2::CourtTirChildChosen { session_id: sid, choice, signature }
}

/// The honest children of the disputed range against the phase's root (what a truthful challenger
/// computes for itself), and the same children pushed to fold to the phase's (possibly false) claim.
fn children_of(w: &World, x: &Execution, phase: &PalwTirDissectPhaseV1) -> (PalwTirDissectRoundV1, PalwTirDissectRoundV1) {
    let store = Store { f: &w.f, x };
    let honest = build_tir_dissect_round_v1(&x.binding, phase, w.site.tile_positions, &store, &RULES).expect("a round");
    let mut pushed = honest.clone();
    for (i, fold) in w.site.folds.iter().enumerate() {
        for e in 0..phase.elements()[i].len() {
            let claim = phase.claim().partials[i][e];
            match fold {
                PalwTirFoldV1::Sum => {
                    let sum: i128 = pushed.children.iter().map(|c| c.partials[i][e]).sum();
                    pushed.children[0].partials[i][e] += claim - sum;
                }
                PalwTirFoldV1::Max => {
                    let max = pushed.children.iter().map(|c| c.partials[i][e]).max().unwrap();
                    if claim > max {
                        pushed.children[0].partials[i][e] = claim;
                    } else {
                        for c in pushed.children.iter_mut() {
                            c.partials[i][e] = c.partials[i][e].min(claim);
                        }
                    }
                }
            }
        }
    }
    (honest, pushed)
}

#[derive(Debug, PartialEq, Eq)]
enum Ending {
    /// The bottom close adjudicated and folded with this verdict.
    Closed(PalwCourtVerdictV2),
    /// A round could not fold: the fold refused it, and the responder has not moved.
    RoundRefused,
}

/// Play the opened phase to its end: every round the responder's children (the truth, or — `push` —
/// the truth pushed to fold to the phase's claim), the challenger naming the first child whose claim
/// is not what it computes (or, when none is, the first and the last in turn), then the bottom close
/// adjudicated as the processor adjudicates it and folded.
fn play(run: &mut Run, w: &World, x: &Execution, sid: Hash64, push: bool) -> Ending {
    let mut played = 0usize;
    while run.phase(&sid).turn() == PalwBisectTurnV1::AwaitDisclosure {
        let phase = run.phase(&sid);
        let (truth, pushed) = children_of(w, x, &phase);
        let filed = if push { pushed.clone() } else { truth.clone() };
        let object = dissected(run, sid, filed.clone());
        assert!(palw_court_move_spends_the_slot_v1(&run.s, &object));
        if let Err(e) = run.try_at(run.daa + 1, std::slice::from_ref(&object), None) {
            assert!(matches!(e, PalwStateV2Error::DissectionRefused(..)), "{e}");
            return Ending::RoundRefused;
        }
        run.step(&[object]);
        let named = filed.children.iter().zip(&truth.children).position(|(f, t)| f != t).unwrap_or_else(|| {
            let last = filed.children.len() - 1;
            if played % 2 == 0 { 0 } else { last }
        }) as u8;
        run.step(&[chosen(run, sid, named)]);
        played += 1;
    }
    let phase = run.phase(&sid);
    assert_eq!(phase.turn(), PalwBisectTurnV1::Terminal, "one tile left: the bottom");
    let bottom = build_tir_dissect_bottom_v1(&x.binding, &phase, &Store { f: &w.f, x }, &RULES).expect("the bottom's carriage");
    let proof = PalwCourtVerdictProofV2::TirDissection { bottom: Box::new(bottom) };
    assert!(palw_object_is_tir_v1(&PalwConsensusObjectV2::CourtClosed {
        session_id: sid,
        verdict: PalwCourtVerdictV2::ExecutorGuilty,
        proof: proof.clone()
    }));
    let verdict = adjudicate_court_close_v3(&run.s, &sid, &proof, &court(), LADDER, PalwPromptIdsFormV1::Flat, false, false)
        .expect("the bottom adjudicates");
    run.step(&[PalwConsensusObjectV2::CourtClosed { session_id: sid, verdict, proof }]);
    assert!(run.s.court_session(&sid).is_none() && run.s.tir_dissection_v1(&sid).is_none(), "the session and its phase end together");
    Ending::Closed(verdict)
}

fn voided_by_the_court(run: &Run, claim_id: &Hash64) -> bool {
    matches!(run.s.claim(claim_id).expect("the claim").phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtFraud, .. })
}

/// A lie of reduction `k`: false totals that finalize, and the execution that committed what they
/// finalize to (the honest tile when the lie is absorbed). The smallest power-of-two lie that moves
/// the tile, else the lie of one the tile absorbs.
fn lie_in(w: &World, k: usize) -> Option<(PalwTirRootClaimV1, Execution, bool)> {
    const STEPS: [i128; 9] = [1, 1 << 4, 1 << 8, 1 << 12, 1 << 16, 1 << 20, 1 << 24, 1 << 28, 1 << 32];
    let li = w.leaf as usize;
    let store = Store { f: &w.f, x: &w.honest };
    let root = build_tir_root_claim_v1(&w.honest.binding, w.leaf, &store, &RULES).expect("the honest root");
    let iv = w.f.interval(&w.f.leaves[li]);
    let n = root.totals.partials[k].len();
    let mut absorbed = None;
    for e in [0, n / 2, n - 1] {
        for step in STEPS {
            for delta in [step, -step] {
                let mut totals = root.totals.clone();
                totals.partials[k][e] += delta;
                if totals.partials[k][e] < w.site.bounds[k].lo || totals.partials[k][e] > w.site.bounds[k].hi {
                    continue;
                }
                let Ok(tile) = tir_root_claim_finalizes_to_v1(&w.honest.binding, w.leaf, &root.elements, &totals, &store, &RULES)
                else {
                    continue;
                };
                if tile.iter().any(|v| !iv.contains(*v)) {
                    continue; // a cone close convicts that tile on its interval, before any dissection
                }
                let moved = tile != w.f.values[li];
                if !moved && absorbed.is_some() {
                    continue;
                }
                let mut values = w.f.values.clone();
                values[li] = tile;
                let x = w.f.commit(&values, &w.f.rows, &w.f.generated);
                let carriage = build_tir_root_claim_v1(&x.binding, w.leaf, &Store { f: &w.f, x: &x }, &RULES).expect("a carriage");
                let lie = PalwTirRootClaimV1 {
                    version: PALW_TIR_DISSECT_OBJECT_VERSION_V1,
                    elements: root.elements.clone(),
                    totals,
                    finalize: carriage.finalize,
                };
                if moved {
                    return Some((lie, x, true));
                }
                absorbed = Some((lie, x, false));
            }
        }
    }
    absorbed
}

// =================================================================================================
// The exit test
// =================================================================================================

#[test]
fn an_honest_executor_is_acquitted_at_the_bottom_whichever_child_is_named() {
    let w = world();
    for arity in [2u8, 4] {
        let (mut run, claim_id) = licensed(&w, &w.honest);
        let sid = court_at_leaf(&mut run, claim_id, &w.honest, w.leaf);
        // The narrowed leaf is dissected: until the root claim, the responder owes the move.
        let root =
            build_tir_root_claim_v1(&w.honest.binding, w.leaf, &Store { f: &w.f, x: &w.honest }, &RULES).expect("the root claim");
        assert_eq!(root.totals.partials.len(), w.site.reductions.len(), "one list per reduction over H");
        run.step(&[root_claimed(&run, sid, root.clone(), arity)]);
        let phase = run.phase(&sid);
        assert_eq!(phase.leaf_index(), w.leaf);
        assert_eq!(phase.arity(), arity);
        assert_eq!(phase.turn(), PalwBisectTurnV1::AwaitDisclosure, "twelve tiles: the responder's round");
        assert_eq!(phase.round_budget(), if arity == 2 { 4 } else { 2 }, "twelve tiles at arity {arity}");
        // A second root claim is not a move while the phase is open.
        assert!(matches!(run.refused(&[root_claimed_unchecked(sid, root, arity)]), PalwStateV2Error::DissectionAlreadyOpen(_)));
        let challenger_before = run.collateral(CHALLENGER);
        assert_eq!(play(&mut run, &w, &w.honest, sid, false), Ending::Closed(PalwCourtVerdictV2::ChallengerDefeated), "arity {arity}");
        assert!(!run.s.claim(&claim_id).expect("the claim").phase.is_terminal(), "the honest claim stands");
        assert!(run.collateral(CHALLENGER) < challenger_before, "the defeated challenger pays for the accusation");
        assert_eq!(run.collateral(PRODUCER), 1_000, "the acquitted executor pays nothing");
    }
}

/// The root claim object without the acceptance layer's checks — for the fold's own refusals.
fn root_claimed_unchecked(sid: Hash64, root: PalwTirRootClaimV1, arity: u8) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::CourtTirRootClaimed { session_id: sid, root: Box::new(root), arity, signature: vec![1; 8] }
}

#[test]
fn a_lie_in_each_reduction_is_convicted_at_the_bottom_or_at_the_fold() {
    let w = world();
    let (mut at_bottom, mut at_fold, mut moved_lies) = (0, 0, 0);
    for k in 0..w.site.reductions.len() {
        let (lie, x, moved) = lie_in(&w, k).unwrap_or_else(|| panic!("reduction {k}: some lie finalizes"));
        moved_lies += usize::from(moved);
        // The liar pushes its lie into a child every round (`push`), or answers the truth — which
        // folds to the honest total, not to its claim.
        for push in [true, false] {
            let (mut run, claim_id) = licensed(&w, &x);
            let sid = court_at_leaf(&mut run, claim_id, &x, w.leaf);
            run.step(&[root_claimed(&run, sid, lie.clone(), 2)]);
            let producer_before = run.collateral(PRODUCER);
            match play(&mut run, &w, &x, sid, push) {
                Ending::Closed(verdict) => {
                    assert!(push, "reduction {k}: the truth never folds to a false claim");
                    assert_eq!(verdict, PalwCourtVerdictV2::ExecutorGuilty, "reduction {k}: the lie is convicted at its tile");
                    at_bottom += 1;
                }
                Ending::RoundRefused => {
                    // A round that does not fold is no move; the responder's clock runs out.
                    let deadline = run.phase(&sid).last_deadline_daa();
                    run.at(deadline + 1, &[], None);
                    at_fold += 1;
                }
            }
            assert!(voided_by_the_court(&run, &claim_id), "reduction {k}: the forged claim is voided");
            assert!(run.collateral(PRODUCER) < producer_before, "reduction {k}: the executor is charged");
            assert!(run.s.court_session(&sid).is_none() && run.s.tir_dissection_v1(&sid).is_none());
        }
        eprintln!("reduction {k} ({:?}): a lie that {} the tile, convicted", w.site.folds[k], if moved { "moved" } else { "left" });
    }
    eprintln!("{at_bottom} at the bottom, {at_fold} by a round that could not fold");
    assert_eq!(at_bottom + at_fold, 2 * w.site.reductions.len());
    assert!(at_bottom >= w.site.reductions.len() - 1, "lies followed down to their tiles");
    assert!(at_fold >= w.site.reductions.len(), "every truthful round under a false root refused at the fold");
    assert!(moved_lies >= 1, "a lie that moved the committed tile");
}

#[test]
fn silence_loses_at_every_turn() {
    let w = world();
    let root = || build_tir_root_claim_v1(&w.honest.binding, w.leaf, &Store { f: &w.f, x: &w.honest }, &RULES).expect("the root");

    // (a) The responder does not file the root claim: the terminal ladder's clock is the responder's.
    let (mut run, claim_id) = licensed(&w, &w.honest);
    let sid = court_at_leaf(&mut run, claim_id, &w.honest, w.leaf);
    let deadline = run.s.court_session(&sid).unwrap().ladder.last_deadline_daa();
    run.at(deadline, &[], None);
    assert!(run.s.court_session(&sid).is_some(), "not before its deadline");
    run.at(deadline + 1, &[], None);
    assert!(voided_by_the_court(&run, &claim_id), "(a) the silent responder loses");

    // (b) The responder files the root claim and then no round.
    let (mut run, claim_id) = licensed(&w, &w.honest);
    let sid = court_at_leaf(&mut run, claim_id, &w.honest, w.leaf);
    run.step(&[root_claimed(&run, sid, root(), 2)]);
    let deadline = run.phase(&sid).last_deadline_daa();
    assert_eq!(deadline, run.daa + TURN, "one rung window");
    run.at(deadline + 1, &[], None);
    assert!(voided_by_the_court(&run, &claim_id), "(b) the silent responder loses");
    assert!(run.s.tir_dissection_v1(&sid).is_none(), "and its phase goes with its session");

    // (c) The challenger names no child.
    let (mut run, claim_id) = licensed(&w, &w.honest);
    let sid = court_at_leaf(&mut run, claim_id, &w.honest, w.leaf);
    run.step(&[root_claimed(&run, sid, root(), 2)]);
    let (truth, _) = children_of(&w, &w.honest, &run.phase(&sid));
    run.step(&[dissected(&run, sid, truth)]);
    assert_eq!(run.phase(&sid).turn(), PalwBisectTurnV1::AwaitVerdict);
    let challenger_before = run.collateral(CHALLENGER);
    let deadline = run.phase(&sid).last_deadline_daa();
    run.at(deadline + 1, &[], None);
    assert!(run.s.court_session(&sid).is_none(), "(c) the session ends");
    assert!(!run.s.claim(&claim_id).unwrap().phase.is_terminal(), "(c) on the challenger's side: the claim stands");
    assert!(run.collateral(CHALLENGER) < challenger_before, "(c) the silent challenger pays");

    // (d) At the bottom nobody closes: the burden is the challenger's, and the backstop ends it there.
    let (mut run, claim_id) = licensed(&w, &w.honest);
    let sid = court_at_leaf(&mut run, claim_id, &w.honest, w.leaf);
    run.step(&[root_claimed(&run, sid, root(), 2)]);
    while run.phase(&sid).turn() == PalwBisectTurnV1::AwaitDisclosure {
        let (truth, _) = children_of(&w, &w.honest, &run.phase(&sid));
        run.step(&[dissected(&run, sid, truth)]);
        run.step(&[chosen(&run, sid, 0)]);
    }
    assert_eq!(run.phase(&sid).turn(), PalwBisectTurnV1::Terminal);
    let backstop = run.s.court_session(&sid).unwrap().deadline_daa;
    run.at(backstop, &[], None);
    assert!(run.s.court_session(&sid).is_some(), "at the bottom only the backstop runs");
    run.at(backstop + 1, &[], None);
    assert!(run.s.court_session(&sid).is_none() && run.s.tir_dissection_v1(&sid).is_none(), "(d) the backstop ends it");
    assert!(!run.s.claim(&claim_id).unwrap().phase.is_terminal(), "(d) on the challenger's side");
}

#[test]
fn the_held_regime_opens_the_phase_from_a_one_move_accusation() {
    let w = world();
    let (lie, x, _) = lie_in(&w, 0).expect("a lie in the first reduction");
    let (mut run, claim_id) = licensed(&w, &x);
    run.extras.held_context_ladder = Some(LADDER);
    // The held regime plays no bisection.
    let size = x.binding.step_leaf_count;
    let opened = PalwConsensusObjectV2::CourtOpened {
        session_id: court_session_id_v2(
            &claim_id,
            &x.binding.full_logits_trace_root,
            &bond_key(PRODUCER),
            &bond_key(CHALLENGER),
            PalwBisectSpaceV1::StepLeaves,
            size,
        ),
        claim: claim_id,
        challenger_bond: bond_key(CHALLENGER),
        space: PalwBisectSpaceV1::StepLeaves,
        space_size: size,
        signature: Vec::new(),
    };
    assert!(matches!(run.refused(&[opened]), PalwStateV2Error::BisectionRefusedUnderHeldContext(_)));

    // The accusation names the leaf and carries nothing else: the acceptance layer routes it to a
    // dissection under the held regime; outside it the same object does not adjudicate.
    let named = build_tir_named_leaf_refutation_v1(&x.binding, w.leaf, &Store { f: &w.f, x: &x }).expect("the named leaf");
    let claim = run.s.claim(&claim_id).expect("live").clone();
    let accuse = |verdict| {
        let mut a = palw_tir_one_move_accusation_v1(
            claim_id,
            &claim,
            bond_key(CHALLENGER),
            verdict,
            PalwCourtVerdictProofV2::TirCone { refutation: Box::new(named.clone()) },
        );
        a.signature = vec![9; 8];
        a
    };
    let a = accuse(PalwCourtVerdictV2::ExecutorGuilty);
    let outcome = |held| palw_tir_one_move_outcome_v1(&run.s, &claim, &a, &court(), LADDER, PalwPromptIdsFormV1::Flat, held);
    assert_eq!(outcome(true), Ok(PalwTirOneMoveOutcomeV1::NeedsDissection { leaf: w.leaf }));
    assert!(outcome(false).is_err(), "outside the held regime a leaf is argued whole, and this carries none of its cone");
    // An accusation that opens a dissection prosecutes; one declaring its own defeat is refused.
    let object = |a| PalwConsensusObjectV2::TirShardCourtAccused { accusation: Box::new(a) };
    assert!(matches!(run.refused(&[object(accuse(PalwCourtVerdictV2::ChallengerDefeated))]), PalwStateV2Error::ShardCourt(_)));

    run.step(&[object(a)]);
    let (&sid, session) = run.s.court_sessions_iter().find(|(_, s)| s.claim == claim_id).expect("a session opened");
    assert_eq!(session.ladder.terminal_index(), Some(w.leaf), "at the named leaf");
    assert_eq!(session.ladder.round(), 0, "no rung was played");
    assert_eq!(session.challenger_bond, bond_key(CHALLENGER));
    run.step(&[root_claimed(&run, sid, lie, 2)]);
    let producer_before = run.collateral(PRODUCER);
    match play(&mut run, &w, &x, sid, true) {
        Ending::Closed(verdict) => assert_eq!(verdict, PalwCourtVerdictV2::ExecutorGuilty),
        Ending::RoundRefused => {
            let deadline = run.phase(&sid).last_deadline_daa();
            run.at(deadline + 1, &[], None);
        }
    }
    assert!(voided_by_the_court(&run, &claim_id), "the forged claim is voided");
    assert!(run.collateral(PRODUCER) < producer_before);
}

#[test]
fn every_move_is_refused_out_of_turn_by_the_wrong_party_and_below_the_fence() {
    let w = world();
    let (mut run, claim_id) = licensed(&w, &w.honest);
    let sid = court_at_leaf(&mut run, claim_id, &w.honest, w.leaf);
    let root = build_tir_root_claim_v1(&w.honest.binding, w.leaf, &Store { f: &w.f, x: &w.honest }, &RULES).expect("the root");

    // Before the root claim: no phase for a round or a choice.
    let round = PalwTirDissectRoundV1 { version: PALW_TIR_DISSECT_OBJECT_VERSION_V1, children: Vec::new() };
    let choice = PalwTirDissectChoiceV1 { version: PALW_TIR_DISSECT_OBJECT_VERSION_V1, session_id: sid, round: 0, child: 0 };
    let early_round = PalwConsensusObjectV2::CourtTirDissected { session_id: sid, round: round.clone(), signature: vec![1; 8] };
    let early_choice = PalwConsensusObjectV2::CourtTirChildChosen { session_id: sid, choice: choice.clone(), signature: vec![1; 8] };
    assert!(matches!(run.refused(&[early_round.clone()]), PalwStateV2Error::NoDissection(_)));
    assert!(matches!(run.refused(&[early_choice.clone()]), PalwStateV2Error::NoDissection(_)));
    assert!(!palw_court_move_spends_the_slot_v1(&run.s, &early_round), "a move the fold refuses on its face spends no slot");
    assert!(
        matches!(check_court_tir_round_acceptance_v1(&run.s, &sid, &round, &[], verify), Err(PalwCourtV2Error::NoDissection(_))),
        "nor does the acceptance layer admit it"
    );

    // The root claim: the responder's key only, the ruleset's arity only, about the narrowed leaf only.
    let message = palw_tir_root_claim_message_v1(&sid, &root);
    let by_the_challenger = sign(&pubkey(CHALLENGER), &message, PALW_COURT_V2_MLDSA87_ATTN_RESPONDER_CONTEXT);
    assert_eq!(
        check_court_tir_root_claim_acceptance_v1(&run.s, &sid, &root, &by_the_challenger, verify),
        Err(PalwCourtV2Error::RungSignatureInvalid)
    );
    let under_the_other_context = sign(&pubkey(PRODUCER), &message, PALW_COURT_V2_MLDSA87_ATTN_CHALLENGER_CONTEXT);
    assert_eq!(
        check_court_tir_root_claim_acceptance_v1(&run.s, &sid, &root, &under_the_other_context, verify),
        Err(PalwCourtV2Error::RungSignatureInvalid)
    );
    assert!(matches!(
        check_court_tir_root_claim_admits_v1(&run.s, &sid, &root, 4, 2, &court(), LADDER, PalwPromptIdsFormV1::Flat),
        Err(PalwCourtV2Error::ArityIsNotTheDerivedOne { declared: 4, derived: 2 })
    ));
    let mut elsewhere = root.clone();
    elsewhere.finalize.output_opening.leaf_index = w.leaf - 1;
    assert!(
        check_court_tir_root_claim_admits_v1(&run.s, &sid, &elsewhere, 2, 2, &court(), LADDER, PalwPromptIdsFormV1::Flat).is_err()
    );
    assert!(matches!(run.refused(&[root_claimed_unchecked(sid, elsewhere, 2)]), PalwStateV2Error::DissectionRefused(..)));
    let mut out_of_bounds = root.clone();
    out_of_bounds.totals.partials[0][0] = w.site.bounds[0].hi + 1;
    assert!(
        check_court_tir_root_claim_admits_v1(&run.s, &sid, &out_of_bounds, 2, 2, &court(), LADDER, PalwPromptIdsFormV1::Flat).is_err()
    );
    assert!(matches!(run.refused(&[root_claimed_unchecked(sid, out_of_bounds, 2)]), PalwStateV2Error::DissectionRefused(..)));
    assert!(
        matches!(run.refused(&[root_claimed_unchecked(sid, root.clone(), 3)]), PalwStateV2Error::DissectionRefused(..)),
        "arity 3"
    );
    // Honest totals do not finalize to a forged tile.
    let li = w.leaf as usize;
    let iv = w.f.interval(&w.f.leaves[li]);
    let mut values = w.f.values.clone();
    values[li][0] = if iv.contains(values[li][0] + 1) { values[li][0] + 1 } else { values[li][0] - 1 };
    let forged = w.f.commit(&values, &w.f.rows, &w.f.generated);
    let (forged_run, forged_claim) = licensed(&w, &forged);
    let mut forged_run = forged_run;
    let forged_sid = court_at_leaf(&mut forged_run, forged_claim, &forged, w.leaf);
    let carriage = build_tir_root_claim_v1(&forged.binding, w.leaf, &Store { f: &w.f, x: &forged }, &RULES).expect("a carriage");
    let honest_totals = PalwTirRootClaimV1 { totals: root.totals.clone(), elements: root.elements.clone(), ..carriage };
    assert!(
        check_court_tir_root_claim_admits_v1(
            &forged_run.s,
            &forged_sid,
            &honest_totals,
            2,
            2,
            &court(),
            LADDER,
            PalwPromptIdsFormV1::Flat
        )
        .is_err(),
        "the honest totals do not finalize to the forged tile"
    );
    // Another claim's execution is not this session's.
    assert!(matches!(
        check_court_tir_root_claim_admits_v1(&run.s, &sid, &honest_totals, 2, 2, &court(), LADDER, PalwPromptIdsFormV1::Flat),
        Err(PalwCourtV2Error::TraceRootMismatch | PalwCourtV2Error::ExecutionRootMismatch)
    ));

    // Opened: out of turn and by the wrong party.
    run.step(&[root_claimed(&run, sid, root, 2)]);
    assert!(matches!(run.refused(&[early_choice]), PalwStateV2Error::DissectionRefused(..)), "a choice before the round");
    let (truth, _) = children_of(&w, &w.honest, &run.phase(&sid));
    let mut bent = truth.clone();
    bent.children[0].partials[0][0] += 1;
    assert!(
        matches!(run.refused(&[dissected(&run, sid, bent)]), PalwStateV2Error::DissectionRefused(..)),
        "a round that does not fold"
    );
    let at = run.phase(&sid).round();
    let by_the_challenger =
        sign(&pubkey(CHALLENGER), &palw_tir_round_message_v1(&sid, at, &truth), PALW_COURT_V2_MLDSA87_ATTN_RESPONDER_CONTEXT);
    assert_eq!(
        check_court_tir_round_acceptance_v1(&run.s, &sid, &truth, &by_the_challenger, verify),
        Err(PalwCourtV2Error::RungSignatureInvalid)
    );
    let at_another_round =
        sign(&pubkey(PRODUCER), &palw_tir_round_message_v1(&sid, at + 1, &truth), PALW_COURT_V2_MLDSA87_ATTN_RESPONDER_CONTEXT);
    assert_eq!(
        check_court_tir_round_acceptance_v1(&run.s, &sid, &truth, &at_another_round, verify),
        Err(PalwCourtV2Error::RungSignatureInvalid),
        "a round signed for another round"
    );
    run.step(&[dissected(&run, sid, truth.clone())]);
    assert!(matches!(run.refused(&[dissected(&run, sid, truth)]), PalwStateV2Error::DissectionRefused(..)), "a second round");
    let choice = PalwTirDissectChoiceV1 { version: PALW_TIR_DISSECT_OBJECT_VERSION_V1, session_id: sid, round: 0, child: 0 };
    let by_the_responder =
        sign(&pubkey(PRODUCER), &palw_tir_choice_message_v1(&choice), PALW_COURT_V2_MLDSA87_ATTN_CHALLENGER_CONTEXT);
    assert_eq!(
        check_court_tir_choice_acceptance_v1(&run.s, &sid, &choice, &by_the_responder, verify),
        Err(PalwCourtV2Error::RungSignatureInvalid)
    );
    let out_of_range = PalwTirDissectChoiceV1 { child: 9, ..choice.clone() };
    assert!(matches!(
        run.refused(&[PalwConsensusObjectV2::CourtTirChildChosen { session_id: sid, choice: out_of_range, signature: vec![1; 8] }]),
        PalwStateV2Error::DissectionRefused(..)
    ));
    let another_round = PalwTirDissectChoiceV1 { round: 1, ..choice.clone() };
    assert!(matches!(
        run.refused(&[PalwConsensusObjectV2::CourtTirChildChosen { session_id: sid, choice: another_round, signature: vec![1; 8] }]),
        PalwStateV2Error::DissectionRefused(..)
    ));
    run.step(&[chosen(&run, sid, 0)]);

    // Below the fence every IR move is refused before an arm reads it; the acceptance layer drops it
    // by name, and past the fence the k-ary court's fence is the acceptance layer's too.
    let below = Run { p: params().with_tir_from_daa(None), s: run.s.clone(), daa: run.daa, extras: run.extras.clone() };
    let (truth, _) = children_of(&w, &w.honest, &run.phase(&sid));
    let move_ = dissected(&run, sid, truth);
    assert!(palw_object_is_tir_v1(&move_) && palw_object_is_tir_dissection_move_v1(&move_));
    assert!(matches!(below.refused(&[move_.clone()]), PalwStateV2Error::TirRegistrationRefused(_)));
    assert_eq!(palw_tir_dissection_move_is_admissible_v1(&move_, false), Err(PalwCourtV2Error::KaryCourtDormant));
    assert_eq!(borsh::to_vec(&move_).unwrap()[0], 65, "tag 65");
    assert_eq!(borsh::to_vec(&root_claimed_unchecked(sid, build_root(&w), 2)).unwrap()[0], 64, "tag 64");
    assert_eq!(
        borsh::to_vec(&PalwConsensusObjectV2::CourtTirChildChosen { session_id: sid, choice, signature: vec![1; 8] }).unwrap()[0],
        66,
        "tag 66"
    );
}

fn build_root(w: &World) -> PalwTirRootClaimV1 {
    build_tir_root_claim_v1(&w.honest.binding, w.leaf, &Store { f: &w.f, x: &w.honest }, &RULES).expect("the root")
}
