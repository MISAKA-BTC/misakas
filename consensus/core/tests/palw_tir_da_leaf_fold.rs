//! **The second IR fence's DA unit on the chain: one committed step leaf of an IR claim, demanded
//! and disclosed** (`Params::palw_tir_fence2`; evidence transport C).
//!
//! An IR class (the corpus's dense GQA model) is registered and claimed, its claim bound to a panel;
//! then, past the fence (R-core+ in force from genesis, as on testnet-12), a bond demands one committed step leaf (`DefaultAccusedTirLeaf`), the
//! fold opens an R-core+ session over it and three leaves drawn from the execution's range, and:
//!
//! * the producer answers every unit with `MaterialDisclosedV2` carrying the leaf
//!   (`PalwTirStepLeafDisclosureV1`): the session is refuted and closes, the claim stands;
//! * a producer that stays silent past `W_disclose` defaults: the claim is voided;
//! * a demand past the execution's leaves, a binding that is not the claim's, a demand on a claim
//!   that is not an IR class's, an answer that is not the leaf, and every move below the fence are
//!   refused.
//!
//! Every block goes through the transition and is checked three ways (the delta re-applies and
//! reverts, and the carriage reloads under its root).
//!
//! Run: `cargo test -p kaspa-consensus-core --test palw_tir_da_leaf_fold`

#[path = "palw_tir_fixture_common.rs"]
#[allow(unused)]
mod fixture;
use fixture::*;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_attempt_v2::{PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, attempt_id_v2, challenge_v2};
use kaspa_consensus_core::palw_da_rcore_v1::{PalwDaAnswerV1, PalwDaUnitV1, PalwTirLeafAccusationV1};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwConsensusObjectV2, PalwPanelSeatV2, PalwPwuRuleV2,
    PalwStateCarriageV2, PalwStateDeltaV2, PalwStateParamsV2, PalwStateV2Error, PalwTransitionExtrasV1, apply_delta_v2,
    apply_palw_transition_v2_with_extras, palw_object_is_tir_fence2_v1, palw_operator_id_v2, revert_delta_v2,
};
use kaspa_consensus_core::palw_tir_class_v1::PalwTirAdmissionCarriageV1;
use kaspa_consensus_core::palw_tir_court_v1::build_tir_step_leaf_disclosure_v1;
use kaspa_consensus_core::palw_tir_step_v1::PalwTirStepBindingV1;
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

const PRODUCER: u64 = 1;
const CHALLENGER: u64 = 2;
/// R-core+ from genesis (as testnet-12 arms it); the second IR fence after the claim is licensed at 4.
const RCORE: u64 = 0;
const FENCE2: u64 = 6;
const WINDOW_CHALLENGE: u64 = 20;
const MAX: u64 = 1 << 26;

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

fn params() -> PalwStateParamsV2 {
    PalwStateParamsV2::new(100, 10, 10, WINDOW_CHALLENGE, 500, 1000, h64(1), 4, 1000, 100, 1000, 0)
        .unwrap()
        .with_fp_quanta(8, 64)
        .unwrap()
        // The work ceiling at 500‰, as testnet-12 sets it: the other half is an accuser's room (A-6).
        .with_fp_exposure_ceiling(500)
        .unwrap()
        .with_tir_from_daa(Some(0))
        .with_rcore_plus_mirrors(Some(RCORE), 0, Vec::new())
        .with_tir_fence2_from_daa(Some(FENCE2))
}

struct Run {
    p: PalwStateParamsV2,
    s: PalwChainStateV2,
    daa: u64,
    extras: PalwTransitionExtrasV1,
}

impl Run {
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

    fn at(&mut self, daa: u64, objects: &[PalwConsensusObjectV2], att: Option<&PalwAttemptEnvelopeV2>) {
        assert!(daa > self.daa, "DAA moves forward");
        let (child, delta) = self.try_at(daa, objects, att).unwrap_or_else(|e| panic!("the block at DAA {daa} folds: {e}"));
        // (Not `assert_internal_consistency`: this harness licenses its claim by the pre-R-core+ door
        // and arms R-core+ after it, a mix the checker's licence-door rule does not model.)
        assert_eq!(apply_delta_v2(&self.s, &delta, &self.p).expect("re-applies"), child, "DAA {daa}: the delta is the transition");
        assert_eq!(revert_delta_v2(&child, &delta, &self.p).expect("reverts"), self.s, "DAA {daa}: the delta reverts");
        let reloaded = PalwStateCarriageV2::from_state(&child)
            .into_state(&self.p, Some(child.state_root()))
            .unwrap_or_else(|e| panic!("DAA {daa}: the carriage reloads: {e}"));
        assert_eq!(reloaded, child, "DAA {daa}: reload is the state");
        self.s = child;
        self.daa = daa;
    }

    fn refused_at(&self, daa: u64, objects: &[PalwConsensusObjectV2]) -> PalwStateV2Error {
        self.try_at(daa, objects, None).expect_err("the fold refuses it")
    }
}

fn bond(n: u64, collateral: u64) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::BondRegistered {
        bond: bond_key(n),
        pubkey: pubkey(n),
        operator_pubkey: op_key(n),
        collateral,
        payout_payload: Hash64::from_u64_word(0x9A00 + n),
        capable_classes: Default::default(),
        signature: Vec::new(),
    }
}

/// The dense GQA corpus model as an IR class, run as a short job.
fn fixture() -> Fixture {
    let (name, program, params, tokens) =
        programs().into_iter().find(|(n, ..)| n == "dense-gqa-2layer").expect("the dense GQA corpus model");
    fixture_with(name, program, params, tokens, 4, 3)
}

/// The chain up to a live claim of the IR class committing `x` (the floor class, both bonds and the IR
/// class at DAA 1, the claim at 2, its panel at 3).
fn claimed(f: &Fixture, x: &Execution) -> (Run, Hash64) {
    let mut run = Run { p: params(), s: PalwChainStateV2::genesis(), daa: 0, extras: PalwTransitionExtrasV1::default() };
    let class_id = f.class_id;
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
            bond(PRODUCER, 1_000_000),
            bond(CHALLENGER, 1_000_000),
            PalwConsensusObjectV2::ClassRegisteredTirV1 {
                class_id,
                artifact_root: f.artifact_root,
                slash_value_per_pwu: 5,
                pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: 40 },
                initial_target: u128::MAX / 2,
                share_permille: 0,
                activation_daa: 0,
                admission: Box::new(PalwTirAdmissionCarriageV1 {
                    class: f.class.clone(),
                    canonical: f.ctx.clone(),
                    registrant_bond: bond_key(PRODUCER),
                    signature: vec![9; 8],
                }),
            },
        ],
        None,
    );
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
            artifact_root: f.artifact_root,
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
    // Bound to its panel, the claim is live — a DA session's stage `Live` (R-core+ accuses a claim at
    // any live stage).
    assert!(matches!(run.s.claim(&claim_id).expect("live").phase, PalwClaimPhaseV2::PanelBound { .. }));
    (run, claim_id)
}

fn stripped(binding: &PalwTirStepBindingV1) -> PalwTirStepBindingV1 {
    let mut b = binding.clone();
    b.class.program = Vec::new();
    b
}

fn demand(claim: Hash64, index: u64, binding: &PalwTirStepBindingV1, accuser: u64) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::DefaultAccusedTirLeaf {
        accusation: Box::new(PalwTirLeafAccusationV1 {
            claim,
            index,
            binding: stripped(binding),
            accuser: bond_key(accuser),
            signature: vec![1; 8],
        }),
    }
}

fn answer(f: &Fixture, x: &Execution, claim: Hash64, index: u64) -> PalwConsensusObjectV2 {
    let store = Store { f, x };
    let disclosure = build_tir_step_leaf_disclosure_v1(&x.binding, index, &store, MAX).unwrap_or_else(|e| panic!("leaf {index}: {e}"));
    PalwConsensusObjectV2::MaterialDisclosedV2 {
        claim,
        unit: PalwDaUnitV1::TirStepLeaf { index },
        answer: PalwDaAnswerV1::TirStepLeaf(Box::new(disclosure)),
        discloser: bond_key(PRODUCER),
        signature: vec![2; 8],
    }
}

/// The units of `accuser`'s open session on `claim`.
fn session_units(run: &Run, claim: &Hash64, accuser: u64) -> Option<Vec<PalwDaUnitV1>> {
    run.s.da_sessions_of(claim).find(|(bond, _)| **bond == bond_key(accuser)).map(|(_, s)| s.units.clone())
}

#[test]
fn a_demanded_leaf_disclosed_refutes_the_session_and_the_claim_stands() {
    let f = fixture();
    let x = f.honest();
    let (mut run, claim_id) = claimed(&f, &x);
    let count = x.binding.step_leaf_count;
    let named = count / 2;
    // Below the fence (past R-core+) the demand is refused by name — the second lock behind the
    // acceptance walk's drop.
    let named_demand = demand(claim_id, named, &x.binding, CHALLENGER);
    assert!(palw_object_is_tir_fence2_v1(&named_demand));
    assert!(matches!(run.refused_at(FENCE2 - 1, std::slice::from_ref(&named_demand)), PalwStateV2Error::TirFence2Refused(_)));
    run.at(FENCE2, &[named_demand], None);
    let units = session_units(&run, &claim_id, CHALLENGER).expect("a session is open");
    assert_eq!(units[0], PalwDaUnitV1::TirStepLeaf { index: named });
    assert!(units.len() == 4 && units.iter().all(|u| matches!(u, PalwDaUnitV1::TirStepLeaf { index } if *index < count)), "{units:?}");
    // The producer answers every unit; the last answer refutes and closes the session.
    let answers: Vec<PalwConsensusObjectV2> = units
        .iter()
        .map(|u| match u {
            PalwDaUnitV1::TirStepLeaf { index } => answer(&f, &x, claim_id, *index),
            _ => unreachable!(),
        })
        .collect();
    run.at(FENCE2 + 1, &answers, None);
    assert!(session_units(&run, &claim_id, CHALLENGER).is_none(), "the session is refuted and closed");
    assert!(matches!(run.s.claim(&claim_id).expect("live").phase, PalwClaimPhaseV2::PanelBound { .. }), "the claim stands");
    for u in &units {
        assert!(run.s.da_claim(&claim_id).expect("a record").answered.contains(u), "{u:?} answered");
    }
}

#[test]
fn a_producer_silent_past_the_window_defaults() {
    let f = fixture();
    let x = f.honest();
    let (mut run, claim_id) = claimed(&f, &x);
    run.at(FENCE2, &[demand(claim_id, 0, &x.binding, CHALLENGER)], None);
    assert!(session_units(&run, &claim_id, CHALLENGER).is_some());
    let deadline = run.s.da_sessions_of(&claim_id).map(|(_, s)| s.deadline_daa).max().expect("a deadline");
    run.at(deadline + 1, &[], None);
    let phase = run.s.claim(&claim_id).map(|c| c.phase.clone());
    assert!(matches!(phase, Some(PalwClaimPhaseV2::Voided { .. })), "a withheld leaf defaults the claim: {phase:?}");
    assert!(session_units(&run, &claim_id, CHALLENGER).is_none(), "the session closed with the default");
}

#[test]
fn a_demand_or_an_answer_in_any_other_shape_is_refused() {
    let f = fixture();
    let x = f.honest();
    let (mut run, claim_id) = claimed(&f, &x);
    run.at(FENCE2, &[], None);
    let next = FENCE2 + 1;
    let count = x.binding.step_leaf_count;
    // Past the execution's leaves.
    assert!(matches!(
        run.refused_at(next, &[demand(claim_id, count, &x.binding, CHALLENGER)]),
        PalwStateV2Error::TirLeafOutOfRange { .. }
    ));
    // Another execution's binding.
    let mut other = x.binding.clone();
    other.full_logits_trace_root = h64(0xBAD);
    assert!(matches!(run.refused_at(next, &[demand(claim_id, 0, &other, CHALLENGER)]), PalwStateV2Error::DaAnswerMalformed { .. }));
    // A claim nobody made.
    assert!(matches!(run.refused_at(next, &[demand(h64(0xDEAD), 0, &x.binding, CHALLENGER)]), PalwStateV2Error::MissingClaim(_)));
    // An answer to a unit no session demands.
    assert!(matches!(run.refused_at(next, &[answer(&f, &x, claim_id, 0)]), PalwStateV2Error::DaUnitNotDemanded(_)));
    // Open a session, then answer its named unit with another leaf's disclosure.
    run.at(next, &[demand(claim_id, 1, &x.binding, CHALLENGER)], None);
    let PalwConsensusObjectV2::MaterialDisclosedV2 { answer: wrong, .. } = answer(&f, &x, claim_id, 2) else { unreachable!() };
    let mismatched = PalwConsensusObjectV2::MaterialDisclosedV2 {
        claim: claim_id,
        unit: PalwDaUnitV1::TirStepLeaf { index: 1 },
        answer: wrong,
        discloser: bond_key(PRODUCER),
        signature: vec![2; 8],
    };
    assert!(matches!(run.refused_at(next + 1, &[mismatched]), PalwStateV2Error::DaOpeningRefused { .. }));
    // An answer whose binding still carries the program.
    let PalwConsensusObjectV2::MaterialDisclosedV2 { answer: PalwDaAnswerV1::TirStepLeaf(mut d), .. } = answer(&f, &x, claim_id, 1) else {
        unreachable!()
    };
    d.binding.class.program = f.class.program.clone();
    let carrying = PalwConsensusObjectV2::MaterialDisclosedV2 {
        claim: claim_id,
        unit: PalwDaUnitV1::TirStepLeaf { index: 1 },
        answer: PalwDaAnswerV1::TirStepLeaf(d),
        discloser: bond_key(PRODUCER),
        signature: vec![2; 8],
    };
    assert!(matches!(run.refused_at(next + 1, &[carrying]), PalwStateV2Error::DaAnswerMalformed { .. }));
    // The honest answer lands.
    run.at(next + 1, &[answer(&f, &x, claim_id, 1)], None);
}
