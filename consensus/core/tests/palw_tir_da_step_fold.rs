//! **The second IR fence's DA units on the chain: an IR claim's step tree, demanded and disclosed**
//! (`Params::palw_tir_fence2`; evidence transport C).
//!
//! An IR class (the corpus's dense GQA model) is registered and claimed, its claim bound to a panel
//! of one seat; then, past the fence (R-core+ in force from genesis, as on testnet-12), a bond
//! demands one unit of the claim's step tree with `DefaultAccusedTirStep` — keyed by the claim alone,
//! no binding, no draws — and the fold opens an R-core+ session naming exactly that unit:
//!
//! * **a step leaf** is answered by the leaf (`PalwTirStepLeafDisclosureV1`), **an interior step
//!   node** by its frontier ten levels down and its opening (`PalwTirStepNodeDisclosureV1`), and a
//!   unit **past the claim's execution** by the claim's binding proving so (`TirStepOutOfRange`): the
//!   session is refuted and closes, the claim stands;
//! * **a withholding liar that answers every demand is convicted by ONE seat inside its four
//!   sessions**: the seat names the root, then the first frontier node its own tree disagrees with,
//!   then that leaf — and from the leaf's disclosure and its own execution alone it builds the cone
//!   close that convicts the claim (`TirShardCourtAccused`), its refuted exposure refunded;
//! * **an inconsistent node answer is refused** — a frontier or opening that does not reach the
//!   committed step root, another node's answer, another execution's binding, an out-of-range proof
//!   of a unit inside the execution — so the liar has no answer, and its silence past `W_disclose`
//!   defaults the claim (voided `ProducerWithholding`, the producer slashed); so is a tree with no
//!   leaf under one of its leaf nodes, reached by a descent that every node answer allowed;
//! * a unit past every execution, a demand on a claim that is not an IR class's, and every move
//!   below the fence are refused.
//!
//! Every block goes through the transition and is checked three ways (the delta re-applies and
//! reverts, and the carriage reloads under its root).
//!
//! Run: `cargo test -p kaspa-consensus-core --test palw_tir_da_step_fold`

#[path = "palw_tir_fixture_common.rs"]
#[allow(unused)]
mod fixture;
use fixture::*;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_artifact::{PalwArtifactOpeningV1, open_artifact_leaf_v1};
use kaspa_consensus_core::palw_attempt_v2::{
    PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, attempt_id_v2, challenge_v2,
};
use kaspa_consensus_core::palw_court_v2::PalwCourtVerdictProofV2;
use kaspa_consensus_core::palw_da_rcore_v1::{PalwDaAnswerV1, PalwDaUnitV1, PalwTirStepAccusationV1};
use kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2;
use kaspa_consensus_core::palw_prompt_ids_v1::{PalwPromptIdsFormV1, PalwPromptIdsOpeningV1};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwConsensusObjectV2, PalwCourtVerdictV2, PalwPanelSeatV2,
    PalwPwuRuleV2, PalwStateCarriageV2, PalwStateDeltaV2, PalwStateParamsV2, PalwStateV2Error, PalwTransitionExtrasV1,
    PalwVoidReasonV2, apply_delta_v2, apply_palw_transition_v2_with_extras, palw_accuser_exposure_v1, palw_da_event_index_v1,
    palw_object_is_tir_fence2_v1, palw_operator_id_v2, revert_delta_v2,
};
use kaspa_consensus_core::palw_step_leg::{
    PalwStepFaultV1, PalwStepOpeningV1, PalwStepTileLeafV1, step_merkle_leaf_v1, step_merkle_node_v1, step_merkle_root_v1,
    step_tile_leaf_hash_v1,
};
use kaspa_consensus_core::palw_step_refute::PalwDecodeTokenPinV1;
use kaspa_consensus_core::palw_tir_class_v1::PalwTirAdmissionCarriageV1;
use kaspa_consensus_core::palw_tir_court_v1::{
    PALW_TIR_STEP_NODE_DEPTH_V1, PalwTirEvidenceStoreV1, PalwTirLogitsConsistencyV1, PalwTirStepLeafDisclosureV1,
    PalwTirStepNodeDisclosureV1, PalwTirTraceEventDisclosureV1, PalwTirTraceLanesV1, build_tir_cone_refutation_v1,
    build_tir_row_node_disclosure_v1, build_tir_step_leaf_disclosure_v1, build_tir_step_node_disclosure_v1,
    check_tir_cone_refutation_v1, check_tir_logits_consistency_v1, palw_tir_step_node_frontier_v1, palw_tir_step_node_parts_v1,
    palw_tir_step_tree_height_v1, palw_tir_step_tree_width_v1, tir_logits_event_disclosure_v1,
};
use kaspa_consensus_core::palw_tir_one_move_v1::{
    palw_tir_one_move_accusation_v1, palw_tir_one_move_shape_v1, palw_tir_one_move_verdict_v1,
};
use kaspa_consensus_core::palw_tir_step_v1::{PalwTirStepBindingV1, palw_tir_execution_root_v1};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

const PRODUCER: u64 = 1;
/// The claim's one panel seat: four sessions on the claim over its life (DA-8).
const SEAT: u64 = 2;
/// A bond off the panel (its own budget: three open, sixteen ever).
const OTHER: u64 = 3;
/// R-core+ from genesis (as testnet-12 arms it); the second IR fence after the claim is bound at 3.
const RCORE: u64 = 0;
const FENCE2: u64 = 6;
const WINDOW_CHALLENGE: u64 = 20;
/// The fold's cap on a binding's leaves (`PALW_STEP_LEG_MAX_LEAVES`).
const MAX: u64 = 1 << 22;
/// The court the acceptance layer re-derives a one-move verdict at (the dissection fold's).
const LADDER: u64 = 1 << 26;

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

    fn collateral(&self, n: u64) -> u64 {
        self.s.bond(&bond_key(n)).expect("the bond").collateral
    }

    /// The units of `accuser`'s open session on `claim`.
    fn session_units(&self, claim: &Hash64, accuser: u64) -> Option<Vec<PalwDaUnitV1>> {
        self.s.da_sessions_of(claim).find(|(bond, _)| **bond == bond_key(accuser)).map(|(_, s)| s.units.clone())
    }

    fn sessions_opened_by(&self, claim: &Hash64, seat: u64) -> u8 {
        self.s.da_claim(claim).and_then(|record| record.opened_by_seat.get(&bond_key(seat)).copied()).unwrap_or(0)
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

/// The dense GQA corpus model as an IR class, run as a job long enough that its step tree is more
/// than ten levels tall (a descent of two node sessions and a leaf session).
fn fixture() -> Fixture {
    let (name, program, params, tokens) =
        programs().into_iter().find(|(n, ..)| n == "dense-gqa-2layer").expect("the dense GQA corpus model");
    fixture_with(name, program, params, tokens, 36, 12)
}

/// A leaf the fixture's store answers (a logits tile at a decode row needs its row's pin, which that
/// store does not keep), searched from `from` toward the first leaf.
fn answerable(f: &Fixture, x: &Execution, from: u64) -> u64 {
    (0..=from)
        .rev()
        .find(|i| build_tir_step_leaf_disclosure_v1(&x.binding, *i, &Store { f, x }, MAX).is_ok())
        .expect("an answerable leaf")
}

/// The chain up to a live claim of the IR class committing `x` (the floor class, three bonds and the
/// IR class at DAA 1, the claim at 2, its panel — one seat, `SEAT` — at 3).
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
            bond(SEAT, 1_000_000),
            bond(OTHER, 1_000_000),
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
    let seats = vec![PalwPanelSeatV2 { bond: bond_key(SEAT), operator_id: palw_operator_id_v2(&op_key(SEAT)) }];
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

/// A demand for `unit` of `claim`'s step tree, filed by `accuser`: the claim and the unit, no binding.
fn demand(claim: Hash64, unit: PalwDaUnitV1, accuser: u64) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::DefaultAccusedTirStep {
        accusation: Box::new(PalwTirStepAccusationV1 { claim, unit, accuser: bond_key(accuser), signature: vec![1; 8] }),
    }
}

fn disclosed(claim: Hash64, unit: PalwDaUnitV1, answer: PalwDaAnswerV1) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::MaterialDisclosedV2 { claim, unit, answer, discloser: bond_key(PRODUCER), signature: vec![2; 8] }
}

/// The accused's answer to a step leaf, from its own execution `x`.
fn leaf_disclosure(f: &Fixture, x: &Execution, index: u64) -> PalwTirStepLeafDisclosureV1 {
    build_tir_step_leaf_disclosure_v1(&x.binding, index, &Store { f, x }, MAX).unwrap_or_else(|e| panic!("leaf {index}: {e}"))
}

fn leaf_answer(f: &Fixture, x: &Execution, claim: Hash64, index: u64) -> PalwConsensusObjectV2 {
    disclosed(claim, PalwDaUnitV1::TirStepLeaf { index }, PalwDaAnswerV1::TirStepLeaf(Box::new(leaf_disclosure(f, x, index))))
}

/// The accused's answer to a step node, from its own execution `x`.
fn node_disclosure(f: &Fixture, x: &Execution, level: u8, index: u64) -> PalwTirStepNodeDisclosureV1 {
    build_tir_step_node_disclosure_v1(&x.binding, level, index, &Store { f, x }, MAX)
        .unwrap_or_else(|e| panic!("node ({level}, {index}): {e}"))
}

fn node_answer(f: &Fixture, x: &Execution, claim: Hash64, level: u8, index: u64) -> PalwConsensusObjectV2 {
    disclosed(
        claim,
        PalwDaUnitV1::TirStepNode { level, index },
        PalwDaAnswerV1::TirStepNode(Box::new(node_disclosure(f, x, level, index))),
    )
}

fn out_of_range(x: &Execution, claim: Hash64, unit: PalwDaUnitV1) -> PalwConsensusObjectV2 {
    disclosed(claim, unit, PalwDaAnswerV1::TirStepOutOfRange(Box::new(stripped(&x.binding))))
}

// =================================================================================================
// The seat's side: its own tree, the descent, and what it knows of the accused's tree at the end
// =================================================================================================

/// An execution's step tree, level by level: `levels[0]` the index-bound leaf nodes, the last the root.
struct Tree {
    levels: Vec<Vec<Hash64>>,
}

impl Tree {
    fn of(hashes: &[Hash64]) -> Self {
        let mut levels = vec![hashes.iter().enumerate().map(|(i, h)| step_merkle_leaf_v1(i as u64, h)).collect::<Vec<_>>()];
        while levels.last().unwrap().len() > 1 {
            let last = levels.last().unwrap();
            let mut next: Vec<Hash64> = last.chunks_exact(2).map(|pair| step_merkle_node_v1(&pair[0], &pair[1])).collect();
            if last.len() % 2 == 1 {
                next.push(*last.last().unwrap());
            }
            levels.push(next);
        }
        Self { levels }
    }

    fn count(&self) -> u64 {
        self.levels[0].len() as u64
    }

    fn height(&self) -> u8 {
        (self.levels.len() - 1) as u8
    }
}

/// **One seat's descent against an accused that answers every demand**: it names the root, then the
/// first node of each answered frontier its own tree disagrees with, then that leaf — each a session
/// of its own, opened once the last one closed (one open session per accuser). Returns the leaf, its
/// answer as the chain carried it, and the sessions the seat opened.
fn descend(run: &mut Run, f: &Fixture, accused: &Execution, claim: Hash64, mine: &Tree) -> (u64, PalwTirStepLeafDisclosureV1, u32) {
    let count = mine.count();
    let mut unit = match mine.height() {
        0 => PalwDaUnitV1::TirStepLeaf { index: 0 },
        height => PalwDaUnitV1::TirStepNode { level: height, index: 0 },
    };
    let mut sessions = 0u32;
    loop {
        run.step(&[demand(claim, unit, SEAT)]);
        sessions += 1;
        assert_eq!(run.session_units(&claim, SEAT), Some(vec![unit]), "the session names exactly the demanded unit: no draws");
        match unit {
            PalwDaUnitV1::TirStepNode { level, index } => {
                let answer = node_answer(f, accused, claim, level, index);
                run.step(std::slice::from_ref(&answer));
                assert!(run.session_units(&claim, SEAT).is_none(), "({level}, {index}): the answer refutes the session and closes it");
                // The seat reads the frontier off the chain and compares it with its own tree.
                let PalwConsensusObjectV2::MaterialDisclosedV2 { answer: PalwDaAnswerV1::TirStepNode(d), .. } = answer else {
                    unreachable!()
                };
                let (below, first, end) = palw_tir_step_node_frontier_v1(count, level, index).expect("an interior node");
                assert_eq!(d.frontier.len() as u64, end - first);
                let own = &mine.levels[below as usize][first as usize..end as usize];
                let k = d
                    .frontier
                    .iter()
                    .zip(own)
                    .position(|(theirs, ours)| theirs != ours)
                    .expect("the node differs, so its frontier does") as u64;
                unit = match below {
                    0 => PalwDaUnitV1::TirStepLeaf { index: first + k },
                    below => PalwDaUnitV1::TirStepNode { level: below, index: first + k },
                };
            }
            PalwDaUnitV1::TirStepLeaf { index } => {
                let answer = leaf_answer(f, accused, claim, index);
                run.step(std::slice::from_ref(&answer));
                assert!(run.session_units(&claim, SEAT).is_none(), "leaf {index}: the answer refutes the session and closes it");
                let PalwConsensusObjectV2::MaterialDisclosedV2 { answer: PalwDaAnswerV1::TirStepLeaf(d), .. } = answer else {
                    unreachable!()
                };
                return (index, *d, sessions);
            }
            _ => unreachable!(),
        }
    }
}

/// **What a seat knows of the accused's step tree once its descent ends at `leaf`** — from its own
/// tree and the leaf's disclosed opening alone, never from any other leaf of the accused's:
///
/// * every node wholly before `leaf` is the seat's own (the descent took the FIRST node that differs
///   at every level, so everything before it agrees);
/// * the nodes on `leaf`'s path follow from the disclosed leaf and its opening;
/// * the right sibling of each node on that path is the opening's own.
///
/// Every opening and every run the cone of `leaf` needs names only such nodes — a sibling adjacent to
/// a node before `leaf` is before it, on its path, or the right sibling of a node on its path — so
/// the accused's leaves after `leaf` may be anything.
struct AccusedView<'a> {
    mine: &'a Tree,
    leaf: u64,
    leaf_hash: Hash64,
    path: Vec<Hash64>,
    right: Vec<Option<Hash64>>,
}

impl<'a> AccusedView<'a> {
    fn new(mine: &'a Tree, opening: &PalwStepOpeningV1) -> Self {
        let count = mine.count();
        let leaf = opening.leaf_index;
        let mut current = step_merkle_leaf_v1(leaf, &opening.leaf_hash);
        let (mut path, mut right) = (Vec::new(), Vec::new());
        let mut siblings = opening.siblings.iter();
        let mut level = 0u8;
        while let Some(width) = palw_tir_step_tree_width_v1(count, level).filter(|w| *w > 1) {
            let position = leaf >> level;
            path.push(current);
            let promoted = width % 2 == 1 && position == width - 1;
            if promoted {
                right.push(None);
            } else {
                let sibling = *siblings.next().expect("the opening walks");
                if position % 2 == 0 {
                    right.push(Some(sibling));
                    current = step_merkle_node_v1(&current, &sibling);
                } else {
                    assert_eq!(
                        sibling,
                        mine.levels[level as usize][(position - 1) as usize],
                        "a left sibling is wholly before the leaf"
                    );
                    right.push(None);
                    current = step_merkle_node_v1(&sibling, &current);
                }
            }
            level += 1;
        }
        path.push(current);
        Self { mine, leaf, leaf_hash: opening.leaf_hash, path, right }
    }

    /// The accused's node `(level, position)`, when the seat knows it.
    fn node(&self, level: u8, position: u64) -> Option<Hash64> {
        let count = self.mine.count();
        let lo = position.checked_shl(u32::from(level))?;
        if lo >= count {
            return None;
        }
        let hi = ((position + 1) << level).min(count);
        let on_path = self.leaf >> level;
        if hi <= self.leaf {
            Some(self.mine.levels[level as usize][position as usize])
        } else if position == on_path {
            Some(self.path[level as usize])
        } else if on_path % 2 == 0 && position == on_path + 1 {
            self.right[level as usize]
        } else {
            None
        }
    }

    /// The accused's opening of leaf `index ≤ leaf` — the walk `step_opening_v1` takes.
    fn opening(&self, index: u64, leaf_hash: Hash64) -> Option<PalwStepOpeningV1> {
        let count = self.mine.count();
        let mut siblings = Vec::new();
        let mut level = 0u8;
        while let Some(width) = palw_tir_step_tree_width_v1(count, level).filter(|w| *w > 1) {
            let position = index >> level;
            if !(width % 2 == 1 && position == width - 1) {
                siblings.push(self.node(level, position ^ 1)?);
            }
            level += 1;
        }
        Some(PalwStepOpeningV1 { leaf_index: index, leaf_hash, siblings })
    }

    /// The accused's siblings of the run `[first, first + len)` — the walk
    /// `step_merkle_range_siblings_v1` takes.
    fn range_siblings(&self, first: u64, len: u64) -> Option<Vec<Hash64>> {
        let count = self.mine.count();
        let (mut a, mut b) = (first, first + len);
        let mut out = Vec::new();
        let mut level = 0u8;
        while let Some(width) = palw_tir_step_tree_width_v1(count, level).filter(|w| *w > 1) {
            if a % 2 == 1 {
                out.push(self.node(level, a - 1)?);
            }
            if b % 2 == 1 && !(width % 2 == 1 && b == width) {
                out.push(self.node(level, b)?);
            }
            a /= 2;
            b = b.div_ceil(2);
            level += 1;
        }
        Some(out)
    }
}

/// **The seat's evidence store at the end of its descent**: its own execution below `leaf`, the
/// accused's disclosed `leaf`, the accused's openings as [`AccusedView`] derives them, the class's
/// public parts, and the claim's ids from the disclosure. Nothing past `leaf`.
struct SeatStore<'a> {
    f: &'a Fixture,
    mine: &'a Execution,
    view: AccusedView<'a>,
    disclosed: PalwTirStepLeafDisclosureV1,
}

impl PalwTirEvidenceStoreV1 for SeatStore<'_> {
    fn step_leaf(&self, index: u64) -> Option<PalwStepTileLeafV1> {
        match index.cmp(&self.view.leaf) {
            std::cmp::Ordering::Less => self.mine.preimages.get(index as usize).cloned(),
            std::cmp::Ordering::Equal => Some(self.disclosed.preimage.clone()),
            std::cmp::Ordering::Greater => None,
        }
    }
    fn step_opening(&self, index: u64) -> Option<PalwStepOpeningV1> {
        match index.cmp(&self.view.leaf) {
            std::cmp::Ordering::Less => self.view.opening(index, self.mine.hashes[index as usize]),
            std::cmp::Ordering::Equal => self.view.opening(index, self.view.leaf_hash),
            std::cmp::Ordering::Greater => None,
        }
    }
    fn step_range_siblings(&self, first: u64, count: u64) -> Option<Vec<Hash64>> {
        (first + count <= self.view.leaf + 1).then(|| self.view.range_siblings(first, count)).flatten()
    }
    fn param_opening(&self, leaf: u32) -> Option<PalwArtifactOpeningV1> {
        open_artifact_leaf_v1(&self.f.ops, leaf)
    }
    fn prompt_token_ids(&self) -> Option<Vec<u32>> {
        Some(self.f.prompt.clone())
    }
    fn prompt_ids_opening(&self, _tile: u32) -> Option<PalwPromptIdsOpeningV1> {
        None
    }
    fn decode_pin(&self) -> Option<PalwDecodeTokenPinV1> {
        Some(self.disclosed.decode.clone())
    }
}

/// A lane of leaf `i` moved by one inside its proven interval, if it can be.
fn forged_lane(f: &Fixture, values: &mut [Vec<i128>], i: usize) -> Option<usize> {
    let leaf = &f.leaves[i];
    let lane = leaf.value_count as usize / 2;
    let v = f.values[i][lane];
    let iv = f.interval(leaf);
    let forged = [v + 1, v - 1].into_iter().find(|w| iv.contains(*w))?;
    values[i][lane] = forged;
    Some(lane)
}

/// **The withholding liar**: it forges one lane of a leaf deep in the tree — one whose leaf answers
/// build and whose cone close convicts in one move — and garbles later leaves too (the last one, and
/// the one right after), so no seat can rebuild its tree as "mine with one leaf replaced". It
/// serves nothing, and answers every demand from this execution.
fn liar(f: &Fixture) -> (u64, usize, Execution) {
    let count = f.leaves.len();
    for i in (count * 2 / 3..count).chain(count / 3..count * 2 / 3) {
        let mut values = f.values.clone();
        let Some(lane) = forged_lane(f, &mut values, i) else { continue };
        let single = f.commit(&values, &f.rows, &f.generated);
        // The leaf answers (a logits tile at a decode row would need its row pin, which this store
        // does not keep) and its cone convicts it whole.
        if build_tir_step_leaf_disclosure_v1(&single.binding, i as u64, &Store { f, x: &single }, MAX).is_err() {
            continue;
        }
        let Ok(r) = build_tir_cone_refutation_v1(&single.binding, i as u64, &Store { f, x: &single }, &RULES) else { continue };
        if !matches!(check_tir_cone_refutation_v1(&r, &RULES), Ok(v) if v.fault == PalwStepFaultV1::ComputationMismatch { value_index: lane as u32 })
        {
            continue;
        }
        for later in [count - 1, i + 1] {
            if later > i && later < count {
                let _ = forged_lane(f, &mut values, later);
            }
        }
        return (i as u64, lane, f.commit(&values, &f.rows, &f.generated));
    }
    panic!("{}: no leaf to forge", f.name);
}

#[test]
fn a_withholding_liar_that_answers_every_demand_is_convicted_by_one_seat_within_its_four_sessions() {
    let f = fixture();
    let honest = f.honest();
    let mine = Tree::of(&honest.hashes);
    let count = mine.count();
    assert_eq!(mine.height(), palw_tir_step_tree_height_v1(count));
    assert!(mine.height() > PALW_TIR_STEP_NODE_DEPTH_V1, "{count} leaves: a tree of {} levels needs two node sessions", mine.height());
    let (lie, lane, accused) = liar(&f);
    assert_ne!(accused.binding.step_merkle_root, honest.binding.step_merkle_root);
    let (mut run, claim_id) = claimed(&f, &accused);
    run.at(FENCE2, &[], None);
    let producer_before = run.collateral(PRODUCER);

    // The descent: one seat, one session per unit, each answered by the liar.
    let (leaf, disclosed, sessions) = descend(&mut run, &f, &accused, claim_id, &mine);
    assert_eq!(leaf, lie, "the descent ends at the forged leaf, the first the seat's own tree disputes");
    let node_sessions = u32::from(mine.height()).div_ceil(u32::from(PALW_TIR_STEP_NODE_DEPTH_V1));
    assert_eq!(sessions, node_sessions + 1, "{} levels: {node_sessions} node sessions and the leaf", mine.height());
    assert!(sessions <= 4, "inside one seat's four sessions");
    eprintln!("{count} leaves, {} levels: the seat reached forged leaf {leaf} in {sessions} sessions", mine.height());
    assert_eq!(u32::from(run.sessions_opened_by(&claim_id, SEAT)), sessions, "each counted on the seat's budget (DA-8)");
    assert!(
        palw_accuser_exposure_v1(&run.s, &bond_key(SEAT)) > 0,
        "every refuted session's exposure is held until the claim resolves"
    );
    assert!(matches!(run.s.claim(&claim_id).expect("live").phase, PalwClaimPhaseV2::PanelBound { .. }), "answered, the claim stands");

    // The close, from the seat's own execution and the chain's answers alone.
    let record = run.s.tir_class_v1(&f.class_id).expect("the IR class").clone();
    let disclosed = disclosed.with_program_v1(&record).expect("the class's program put back");
    let store = SeatStore { f: &f, mine: &honest, view: AccusedView::new(&mine, &disclosed.opening), disclosed: disclosed.clone() };
    assert_eq!(store.view.path.last(), Some(&accused.binding.step_merkle_root), "the disclosed leaf walks to the committed root");
    let refutation = build_tir_cone_refutation_v1(&disclosed.binding, leaf, &store, &RULES)
        .unwrap_or_else(|e| panic!("leaf {leaf}: the seat builds the close from its store: {e}"));
    let verdict = check_tir_cone_refutation_v1(&refutation, &RULES).expect("the close adjudicates");
    assert_eq!(verdict.fault, PalwStepFaultV1::ComputationMismatch { value_index: lane as u32 }, "the forged lane");

    // It rides as a one-move accusation; the acceptance layer's re-derivation says guilty, the fold
    // voids the claim for fraud, and the seat's refuted exposure comes back.
    let claim = run.s.claim(&claim_id).expect("live").clone();
    let mut accusation = palw_tir_one_move_accusation_v1(
        claim_id,
        &claim,
        bond_key(SEAT),
        PalwCourtVerdictV2::ExecutorGuilty,
        PalwCourtVerdictProofV2::TirCone { refutation: Box::new(refutation) },
    );
    accusation.signature = vec![9; 8];
    palw_tir_one_move_shape_v1(&accusation).expect("the accusation's shape");
    let court = PalwCourtParamsV2::new(LADDER, 20, 2).expect("a court");
    assert_eq!(
        palw_tir_one_move_verdict_v1(&run.s, &claim, &accusation, &court, LADDER, PalwPromptIdsFormV1::Flat),
        Ok(PalwCourtVerdictV2::ExecutorGuilty),
        "the acceptance layer re-derives the verdict"
    );
    run.step(&[PalwConsensusObjectV2::TirShardCourtAccused { accusation: Box::new(accusation) }]);
    assert!(
        matches!(
            run.s.claim(&claim_id).map(|c| &c.phase),
            Some(PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtFraud, .. })
        ),
        "the forged claim is voided for fraud"
    );
    assert!(run.collateral(PRODUCER) < producer_before, "and its producer slashed");
    assert_eq!(palw_accuser_exposure_v1(&run.s, &bond_key(SEAT)), 0, "the seat's refuted exposure is refunded at the conviction");
}

/// The accused's tree with node `(level, index)`'s answer altered by `edit` — which must not reach the
/// committed root.
fn tampered(
    f: &Fixture,
    x: &Execution,
    claim: Hash64,
    level: u8,
    index: u64,
    edit: impl Fn(&mut PalwTirStepNodeDisclosureV1),
) -> PalwConsensusObjectV2 {
    let mut d = node_disclosure(f, x, level, index);
    edit(&mut d);
    disclosed(claim, PalwDaUnitV1::TirStepNode { level, index }, PalwDaAnswerV1::TirStepNode(Box::new(d)))
}

#[test]
fn an_inconsistent_node_answer_is_refused_and_the_liar_defaults() {
    let f = fixture();
    let honest = f.honest();
    let mine = Tree::of(&honest.hashes);
    let (_, _, accused) = liar(&f);
    let liars_tree = Tree::of(&accused.hashes);
    let (mut run, claim_id) = claimed(&f, &accused);
    run.at(FENCE2, &[], None);
    let top = mine.height();
    assert!(top > PALW_TIR_STEP_NODE_DEPTH_V1, "the root's frontier is interior nodes");
    // The root is answered; the node under it the seat disputes is demanded next.
    run.step(&[demand(claim_id, PalwDaUnitV1::TirStepNode { level: top, index: 0 }, SEAT)]);
    run.step(&[node_answer(&f, &accused, claim_id, top, 0)]);
    let (below, first, end) = palw_tir_step_node_frontier_v1(mine.count(), top, 0).unwrap();
    let k = (first..end).find(|p| mine.levels[below as usize][*p as usize] != liars_tree.levels[below as usize][*p as usize]).unwrap();
    let (level, index) = (below, k);
    run.step(&[demand(claim_id, PalwDaUnitV1::TirStepNode { level, index }, SEAT)]);
    let producer_before = run.collateral(PRODUCER);
    let refused_opening = |object: PalwConsensusObjectV2, what: &str| {
        let e = run.refused(&[object]);
        assert!(matches!(e, PalwStateV2Error::DaOpeningRefused { .. } | PalwStateV2Error::DaAnswerMalformed { .. }), "{what}: {e}");
    };
    // A frontier hash flipped, one dropped, one added, two swapped.
    refused_opening(tampered(&f, &accused, claim_id, level, index, |d| d.frontier[0] = h64(0xF00)), "a frontier node flipped");
    refused_opening(
        tampered(&f, &accused, claim_id, level, index, |d| {
            d.frontier.pop();
        }),
        "a frontier node dropped",
    );
    refused_opening(tampered(&f, &accused, claim_id, level, index, |d| d.frontier.push(h64(0xF01))), "a frontier node added");
    if node_disclosure(&f, &accused, level, index).frontier.len() > 1 {
        refused_opening(tampered(&f, &accused, claim_id, level, index, |d| d.frontier.swap(0, 1)), "two frontier nodes swapped");
    }
    // The frontier of the seat's own (honest) tree under the same node, with the accused's opening.
    refused_opening(
        tampered(&f, &accused, claim_id, level, index, |d| {
            let (b, lo, hi) = palw_tir_step_node_frontier_v1(mine.count(), level, index).unwrap();
            d.frontier = mine.levels[b as usize][lo as usize..hi as usize].to_vec();
        }),
        "the honest frontier",
    );
    // An opening sibling flipped, dropped, added.
    refused_opening(tampered(&f, &accused, claim_id, level, index, |d| d.siblings[0] = h64(0xF02)), "a sibling flipped");
    refused_opening(
        tampered(&f, &accused, claim_id, level, index, |d| {
            d.siblings.pop();
        }),
        "a sibling dropped",
    );
    refused_opening(tampered(&f, &accused, claim_id, level, index, |d| d.siblings.push(h64(0xF03))), "a sibling added");
    // Another node's answer, for the demanded node.
    let neighbour = if index > 0 { index - 1 } else { index + 1 };
    let PalwConsensusObjectV2::MaterialDisclosedV2 { answer: other, .. } = node_answer(&f, &accused, claim_id, level, neighbour)
    else {
        unreachable!()
    };
    refused_opening(disclosed(claim_id, PalwDaUnitV1::TirStepNode { level, index }, other), "a neighbour's answer");
    // The honest execution's answer (another execution's binding).
    refused_opening(node_answer(&f, &honest, claim_id, level, index), "another execution's answer");
    // An out-of-range proof of a node inside the execution.
    refused_opening(
        out_of_range(&accused, claim_id, PalwDaUnitV1::TirStepNode { level, index }),
        "an out-of-range proof of a node inside",
    );
    // A binding that still carries the program.
    let PalwConsensusObjectV2::MaterialDisclosedV2 { answer: PalwDaAnswerV1::TirStepNode(mut d), .. } =
        node_answer(&f, &accused, claim_id, level, index)
    else {
        unreachable!()
    };
    d.binding.class.program = f.class.program.clone();
    refused_opening(
        disclosed(claim_id, PalwDaUnitV1::TirStepNode { level, index }, PalwDaAnswerV1::TirStepNode(d)),
        "the program carried",
    );
    // So the liar has no answer: its silence past W_disclose defaults the claim.
    let deadline = run.s.da_sessions_of(&claim_id).map(|(_, s)| s.deadline_daa).max().expect("the session's deadline");
    run.at(deadline + 1, &[], None);
    assert!(
        matches!(
            run.s.claim(&claim_id).map(|c| &c.phase),
            Some(PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::ProducerWithholding, .. })
        ),
        "a node it cannot answer defaults the claim"
    );
    assert!(run.collateral(PRODUCER) < producer_before, "and slashes its producer");
    assert_eq!(palw_accuser_exposure_v1(&run.s, &bond_key(SEAT)), 0, "a default is a conviction: the seat's exposure comes back");
}

#[test]
fn a_tree_with_no_leaf_under_a_leaf_node_defaults_at_that_leaf() {
    let f = fixture();
    let honest = f.honest();
    let mine = Tree::of(&honest.hashes);
    // The accused commits a tree whose leaf node `gap` hashes no preimage at all: every node answer
    // above it is consistent, and the descent reaches it.
    let gap = (honest.hashes.len() as u64 * 3 / 4..honest.hashes.len() as u64)
        .find(|g| build_tir_step_leaf_disclosure_v1(&honest.binding, *g, &Store { f: &f, x: &honest }, MAX).is_ok())
        .expect("a leaf the fixture's store answers");
    let mut accused = honest.clone();
    accused.hashes[gap as usize] = h64(0x6A9);
    let root = step_merkle_root_v1(&accused.hashes).unwrap();
    let ctx_hash = f.ctx.context_hash();
    accused.binding.step_merkle_root = root;
    accused.binding.committed_execution_root = palw_tir_execution_root_v1(
        &ctx_hash,
        &accused.binding.full_logits_trace_root,
        &f.class_id,
        accused.binding.step_leaf_count,
        &root,
    );
    let (mut run, claim_id) = claimed(&f, &accused);
    run.at(FENCE2, &[], None);
    let count = mine.count();
    let theirs = Tree::of(&accused.hashes);
    let mut unit = PalwDaUnitV1::TirStepNode { level: mine.height(), index: 0 };
    let mut sessions = 0;
    while let PalwDaUnitV1::TirStepNode { level, index } = unit {
        run.step(&[demand(claim_id, unit, SEAT)]);
        run.step(&[node_answer(&f, &accused, claim_id, level, index)]);
        sessions += 1;
        let (below, first, end) = palw_tir_step_node_frontier_v1(count, level, index).unwrap();
        let k = (first..end).find(|p| mine.levels[below as usize][*p as usize] != theirs.levels[below as usize][*p as usize]).unwrap();
        unit = if below == 0 { PalwDaUnitV1::TirStepLeaf { index: k } } else { PalwDaUnitV1::TirStepNode { level: below, index: k } };
    }
    assert_eq!(unit, PalwDaUnitV1::TirStepLeaf { index: gap }, "the descent reaches the leaf node with nothing under it");
    run.step(&[demand(claim_id, unit, SEAT)]);
    assert!(sessions + 1 <= 4);
    // No preimage opens there: the honest leaf under the committed opening is refused.
    assert!(build_tir_step_leaf_disclosure_v1(&accused.binding, gap, &Store { f: &f, x: &accused }, MAX).is_err(), "no answer builds");
    let mut forced = leaf_disclosure(&f, &honest, gap);
    forced.binding = stripped(&accused.binding);
    forced.opening.siblings = kaspa_consensus_core::palw_step_leg::step_opening_v1(&accused.hashes, gap).unwrap().siblings;
    let e = run.refused(&[disclosed(claim_id, unit, PalwDaAnswerV1::TirStepLeaf(Box::new(forced)))]);
    assert!(matches!(e, PalwStateV2Error::DaOpeningRefused { .. }), "{e}");
    let deadline = run.s.da_sessions_of(&claim_id).map(|(_, s)| s.deadline_daa).max().expect("the session's deadline");
    run.at(deadline + 1, &[], None);
    assert!(
        matches!(
            run.s.claim(&claim_id).map(|c| &c.phase),
            Some(PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::ProducerWithholding, .. })
        ),
        "the leaf it cannot answer defaults the claim"
    );
}

#[test]
fn demanded_leaves_and_nodes_disclosed_refute_their_sessions_and_the_claim_stands() {
    let f = fixture();
    let x = f.honest();
    let tree = Tree::of(&x.hashes);
    let (mut run, claim_id) = claimed(&f, &x);
    let count = x.binding.step_leaf_count;
    let middle = answerable(&f, &x, count / 2);
    let named = PalwDaUnitV1::TirStepLeaf { index: middle };
    // Below the fence (past R-core+) the demand is refused by name — the second lock behind the
    // acceptance walk's drop — and so is an answer of the fence's kinds.
    let named_demand = demand(claim_id, named, OTHER);
    assert!(palw_object_is_tir_fence2_v1(&named_demand));
    assert!(matches!(run.try_at(FENCE2 - 1, std::slice::from_ref(&named_demand), None), Err(PalwStateV2Error::TirFence2Refused(_))));
    assert!(palw_object_is_tir_fence2_v1(&leaf_answer(&f, &x, claim_id, 0)));
    assert!(palw_object_is_tir_fence2_v1(&node_answer(&f, &x, claim_id, 1, 0)));
    assert!(palw_object_is_tir_fence2_v1(&out_of_range(&x, claim_id, PalwDaUnitV1::TirStepLeaf { index: count })));
    run.at(FENCE2, &[named_demand], None);
    assert_eq!(run.session_units(&claim_id, OTHER), Some(vec![named]), "named only: no draws");
    run.step(&[leaf_answer(&f, &x, claim_id, middle)]);
    assert!(run.session_units(&claim_id, OTHER).is_none(), "the session is refuted and closed");
    assert!(matches!(run.s.claim(&claim_id).expect("live").phase, PalwClaimPhaseV2::PanelBound { .. }), "the claim stands");
    // A replay of the answered demand opens nothing (F3).
    assert!(matches!(run.refused(&[demand(claim_id, named, OTHER)]), PalwStateV2Error::DaUnitAlreadyAnswered(_)));
    // Interior nodes at every height of the tree are answered — each block answers the last demand
    // and files the next (a bond off the panel pauses nothing, so the claim's own clock runs on).
    let mut units: Vec<PalwDaUnitV1> = Vec::new();
    for level in [1, 8, 9, tree.height()] {
        let Some(width) = palw_tir_step_tree_width_v1(count, level) else { continue };
        for index in [0, width / 2, width - 1] {
            let unit = PalwDaUnitV1::TirStepNode { level, index };
            if !units.contains(&unit) {
                units.push(unit);
            }
        }
    }
    let mut last: Option<PalwDaUnitV1> = None;
    for unit in units.iter().copied().map(Some).chain([None]) {
        let mut block = Vec::new();
        if let Some(PalwDaUnitV1::TirStepNode { level, index }) = last {
            block.push(node_answer(&f, &x, claim_id, level, index));
        }
        if let Some(unit) = unit {
            block.push(demand(claim_id, unit, OTHER));
        }
        run.step(&block);
        assert_eq!(run.session_units(&claim_id, OTHER), unit.map(|u| vec![u]), "{last:?} refuted and closed, {unit:?} open");
        last = unit;
    }
    for unit in &units {
        assert!(run.s.da_claim(&claim_id).expect("a record").answered.contains(unit), "{unit:?} answered");
    }
    // Answered, nothing defaulted it (its own receipt window may have sent it back to be redrawn
    // meanwhile: a bond off the panel pauses nothing).
    let phase = run.s.claim(&claim_id).map(|c| c.phase.clone());
    assert!(!matches!(phase, Some(PalwClaimPhaseV2::Voided { .. })), "the claim stands: {phase:?} at DAA {}", run.daa);
    assert_eq!(run.sessions_opened_by(&claim_id, SEAT), 0, "a bond off the panel spends its own budget");
}

#[test]
fn a_unit_past_the_execution_is_answered_by_the_binding_and_past_every_execution_refused() {
    let f = fixture();
    let x = f.honest();
    let (mut run, claim_id) = claimed(&f, &x);
    run.at(FENCE2, &[], None);
    let count = x.binding.step_leaf_count;
    let top = palw_tir_step_tree_height_v1(count);
    let (_, _, other) = liar(&f);
    // Past every execution a binding may commit: refused at the door, no session opened.
    for unit in [
        PalwDaUnitV1::TirStepLeaf { index: MAX },
        PalwDaUnitV1::TirStepNode { level: 23, index: 0 },
        PalwDaUnitV1::TirStepNode { level: 22, index: 1 },
        PalwDaUnitV1::TirStepNode { level: 0, index: 0 },
        PalwDaUnitV1::Event { row: 0, tile: 0 },
    ] {
        let e = run.refused(&[demand(claim_id, unit, OTHER)]);
        assert!(matches!(e, PalwStateV2Error::TirFence2Refused(_)), "{unit:?}: {e}");
    }
    // A claim nobody made, and a claim that is not an IR class's (the floor class's has no IR record)
    // are refused.
    assert!(matches!(
        run.refused(&[demand(h64(0xDEAD), PalwDaUnitV1::TirStepLeaf { index: 0 }, OTHER)]),
        PalwStateV2Error::MissingClaim(_)
    ));
    // Past this claim's execution, inside the widest: the accused proves it with its binding.
    for unit in [
        PalwDaUnitV1::TirStepLeaf { index: count },
        PalwDaUnitV1::TirStepNode { level: top + 1, index: 0 },
        PalwDaUnitV1::TirStepNode { level: top, index: 1 },
        PalwDaUnitV1::TirStepNode { level: 1, index: palw_tir_step_tree_width_v1(count, 1).unwrap() },
    ] {
        run.step(&[demand(claim_id, unit, OTHER)]);
        assert_eq!(run.session_units(&claim_id, OTHER), Some(vec![unit]));
        // Another execution's binding proves nothing about this claim.
        let e = run.refused(&[out_of_range(&other, claim_id, unit)]);
        assert!(matches!(e, PalwStateV2Error::DaOpeningRefused { .. } | PalwStateV2Error::DaAnswerMalformed { .. }), "{unit:?}: {e}");
        run.step(&[out_of_range(&x, claim_id, unit)]);
        assert!(run.session_units(&claim_id, OTHER).is_none(), "{unit:?}: the proof refutes the session");
        assert!(run.s.da_claim(&claim_id).expect("a record").answered.contains(&unit));
    }
    // An out-of-range proof of a unit inside the execution is refused; its disclosure answers.
    for unit in
        [PalwDaUnitV1::TirStepLeaf { index: answerable(&f, &x, count - 1) }, PalwDaUnitV1::TirStepNode { level: top, index: 0 }]
    {
        run.step(&[demand(claim_id, unit, OTHER)]);
        let e = run.refused(&[out_of_range(&x, claim_id, unit)]);
        assert!(matches!(e, PalwStateV2Error::DaOpeningRefused { .. }), "{unit:?}: {e}");
        run.step(&[match unit {
            PalwDaUnitV1::TirStepLeaf { index } => leaf_answer(&f, &x, claim_id, index),
            PalwDaUnitV1::TirStepNode { level, index } => node_answer(&f, &x, claim_id, level, index),
            _ => unreachable!(),
        }]);
        assert!(run.session_units(&claim_id, OTHER).is_none());
    }
    // An answer to a unit no session demands, and a leaf answered in a node's form, are refused.
    assert!(matches!(run.refused(&[leaf_answer(&f, &x, claim_id, 0)]), PalwStateV2Error::DaUnitNotDemanded(_)));
    run.step(&[demand(claim_id, PalwDaUnitV1::TirStepLeaf { index: 1 }, OTHER)]);
    let PalwConsensusObjectV2::MaterialDisclosedV2 { answer: node_form, .. } = node_answer(&f, &x, claim_id, 1, 0) else {
        unreachable!()
    };
    let e = run.refused(&[disclosed(claim_id, PalwDaUnitV1::TirStepLeaf { index: 1 }, node_form)]);
    assert!(matches!(e, PalwStateV2Error::DaAnswerMalformed { .. }), "{e}");
    run.step(&[leaf_answer(&f, &x, claim_id, 1)]);
}

#[test]
fn a_producer_silent_past_the_window_defaults() {
    let f = fixture();
    let x = f.honest();
    let (mut run, claim_id) = claimed(&f, &x);
    let top = palw_tir_step_tree_height_v1(x.binding.step_leaf_count);
    run.at(FENCE2, &[demand(claim_id, PalwDaUnitV1::TirStepNode { level: top, index: 0 }, SEAT)], None);
    assert!(run.session_units(&claim_id, SEAT).is_some());
    let deadline = run.s.da_sessions_of(&claim_id).map(|(_, s)| s.deadline_daa).max().expect("a deadline");
    run.at(deadline + 1, &[], None);
    let phase = run.s.claim(&claim_id).map(|c| c.phase.clone());
    assert!(matches!(phase, Some(PalwClaimPhaseV2::Voided { .. })), "a withheld node defaults the claim: {phase:?}");
    assert!(run.session_units(&claim_id, SEAT).is_none(), "the session closed with the default");
}

/// The prover's frontier and opening are the tree's own: every node of the fixture's tree at every
/// level folds from its frontier and walks to the root, and the leaf-node frontier is the leaf nodes.
#[test]
fn every_node_of_the_fixture_tree_is_answered_from_its_own_tree() {
    let f = fixture();
    let x = f.honest();
    let tree = Tree::of(&x.hashes);
    let count = tree.count();
    for level in 1..=tree.height() {
        let width = palw_tir_step_tree_width_v1(count, level).unwrap();
        assert_eq!(width, tree.levels[level as usize].len() as u64);
        for index in 0..width {
            let (frontier, _) = palw_tir_step_node_parts_v1(&x.hashes, level, index).expect("in the tree");
            let (below, first, end) = palw_tir_step_node_frontier_v1(count, level, index).unwrap();
            assert_eq!(frontier, tree.levels[below as usize][first as usize..end as usize], "({level}, {index})");
            node_disclosure(&f, &x, level, index);
        }
        assert!(palw_tir_step_node_parts_v1(&x.hashes, level, width).is_none(), "past the level");
    }
    assert!(palw_tir_step_node_parts_v1(&x.hashes, tree.height() + 1, 0).is_none(), "above the root");
    assert!(palw_tir_step_node_parts_v1(&x.hashes, 0, 0).is_none(), "a leaf is not a node");
    // The index-bound leaf node, as the tree hashes it.
    let leaf = &x.preimages[0];
    assert_eq!(tree.levels[0][0], step_merkle_leaf_v1(0, &step_tile_leaf_hash_v1(&f.ctx.context_hash(), &f.class_id, leaf)));
}

// =================================================================================================
// The trace's rows tree: a lie only in the trace, reached by one seat
// =================================================================================================

fn row_answer(f: &Fixture, x: &Execution, claim: Hash64, level: u8, index: u64) -> PalwConsensusObjectV2 {
    let d = build_tir_row_node_disclosure_v1(&x.binding, level, index, &Store { f, x }, MAX)
        .unwrap_or_else(|e| panic!("rows node ({level}, {index}): {e}"));
    disclosed(claim, PalwDaUnitV1::TirRowNode { level, index }, PalwDaAnswerV1::TirRowNode(Box::new(d)))
}

/// An event demand past R-core+ (`DefaultAccused`): row `row`, logits tile `tile` of the trace.
fn event_demand(claim: Hash64, row: u32, tile: u8, accuser: u64) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::DefaultAccused {
        claim,
        missing_event_index: palw_da_event_index_v1(row, tile),
        accuser: bond_key(accuser),
        signature: vec![3; 8],
    }
}

/// The accused's IR event disclosure of `(row, tile)`, program stripped.
fn event_disclosure(x: &Execution, row: u32, tile: u8) -> PalwTirTraceEventDisclosureV1 {
    let mut d = tir_logits_event_disclosure_v1(&x.binding, &x.rows, &x.generated, row, tile).expect("an event of the run");
    d.strip_program_v1();
    d
}

/// The honest run's row roots — the rows tree's leaves.
fn row_roots(f: &Fixture, rows: &[Vec<i32>]) -> Vec<Hash64> {
    let ctx_hash = f.ctx.context_hash();
    rows.iter()
        .enumerate()
        .map(|(r, row)| kaspa_consensus_core::palw_step_refute::tiled_logits_row_root_v1(&ctx_hash, r as u32, row).expect("a row"))
        .collect()
}

/// **A liar whose lie is only in its trace is convicted by one seat inside its four sessions.** The
/// accused commits the honest step tree and a trace whose row `r` differs at one lane (not the
/// selected one: the ids stand). The seat sees the claim's trace root is not its own while its own step
/// root, put beside the claim's trace root, gives the claim's execution root — the steps agree, the
/// trace does not — so it descends the trace's rows tree: the root (`TirRowNode`), then the row its own
/// tree disagrees with, whose tile leaves name the tile; that tile's lanes it demands as an event unit.
/// The liar answers every demand. The step tree's own logits leaf of row `r` beside the disclosed trace
/// tile convicts (`TirLogits`, one move): the claim is voided for fraud and the seat's refuted exposure
/// refunded.
#[test]
fn a_trace_only_liar_is_convicted_by_one_seat_down_the_rows_tree() {
    let f = fixture();
    let honest = f.honest();
    let decode = f.rows.len();
    let r = decode / 2;
    let selected = f.generated[r] as usize;
    let top = *f.rows[r].iter().max().expect("a row");
    let lane = (0..f.rows[r].len()).find(|l| *l != selected && f.rows[r][*l] + 1 < top).expect("a lane under the row's maximum");
    let mut rows = f.rows.clone();
    rows[r][lane] += 1;
    let accused = f.commit(&f.values, &rows, &f.generated);
    assert_eq!(accused.binding.step_merkle_root, honest.binding.step_merkle_root, "the steps agree");
    assert_ne!(accused.binding.full_logits_trace_root, honest.binding.full_logits_trace_root, "the trace does not");
    let (mut run, claim_id) = claimed(&f, &accused);
    run.at(FENCE2, &[], None);
    let claim = run.s.claim(&claim_id).expect("live").clone();
    // The seat's reading: its own steps with the claim's trace root give the claim's execution root.
    assert_eq!(
        kaspa_consensus_core::palw_tir_step_v1::palw_tir_execution_root_v1(
            &f.ctx.context_hash(),
            &claim.trace_root,
            &f.class_id,
            honest.binding.step_leaf_count,
            &honest.binding.step_merkle_root,
        ),
        claim.execution_root,
        "the lie is in the trace alone"
    );
    // Down the rows tree.
    let own_roots = row_roots(&f, &f.rows);
    let own = Tree::of(&own_roots);
    let height = palw_tir_step_tree_height_v1(decode as u64);
    let mut sessions = 0u32;
    let mut unit = PalwDaUnitV1::TirRowNode { level: height, index: 0 };
    let (row, tile) = loop {
        run.step(&[demand(claim_id, unit, SEAT)]);
        sessions += 1;
        assert_eq!(run.session_units(&claim_id, SEAT), Some(vec![unit]), "named only");
        let PalwDaUnitV1::TirRowNode { level, index } = unit else { unreachable!() };
        let answer = row_answer(&f, &accused, claim_id, level, index);
        run.step(std::slice::from_ref(&answer));
        assert!(run.session_units(&claim_id, SEAT).is_none(), "rows node ({level}, {index}): refuted and closed");
        let PalwConsensusObjectV2::MaterialDisclosedV2 { answer: PalwDaAnswerV1::TirRowNode(d), .. } = answer else { unreachable!() };
        if level == 0 {
            // The row's tile leaves: the first the seat's own row disagrees with.
            let ctx_hash = f.ctx.context_hash();
            let own_tiles: Vec<Hash64> = f.rows[index as usize]
                .chunks(kaspa_consensus_core::palw_step_refute::PALW_LOGITS_TILE_LANES)
                .enumerate()
                .map(|(t, lanes)| {
                    kaspa_consensus_core::palw_step_refute::tiled_logits_tile_leaf_v1(&ctx_hash, index as u32, t as u32, lanes)
                })
                .collect();
            let t = d.frontier.iter().zip(&own_tiles).position(|(a, b)| a != b).expect("the row differs, so a tile does");
            break (index as u32, t as u8);
        }
        let (below, first, end) = palw_tir_step_node_frontier_v1(decode as u64, level, index).expect("a node");
        let k = (first..end)
            .find(|p| own.levels[below as usize][*p as usize] != d.frontier[(*p - first) as usize])
            .expect("a node differs")
            - first;
        unit = PalwDaUnitV1::TirRowNode { level: below, index: first + k };
    };
    assert_eq!(row as usize, r, "the descent names the forged row");
    // The tile's lanes, as an event unit: the liar answers the session's every unit.
    run.step(&[event_demand(claim_id, row, tile, SEAT)]);
    sessions += 1;
    let units = run.session_units(&claim_id, SEAT).expect("an event session");
    let answers: Vec<PalwConsensusObjectV2> = units
        .iter()
        .map(|u| {
            let PalwDaUnitV1::Event { row, tile } = *u else { panic!("event units: {u:?}") };
            disclosed(claim_id, *u, PalwDaAnswerV1::TirEvent(Box::new(event_disclosure(&accused, row, tile))))
        })
        .collect();
    run.step(&answers);
    assert!(run.session_units(&claim_id, SEAT).is_none(), "the event session is refuted and closed");
    assert!(sessions <= 4, "inside one seat's four sessions: {sessions}");
    assert_eq!(u32::from(run.sessions_opened_by(&claim_id, SEAT)), sessions);
    eprintln!("{decode} rows: the seat reached row {row} tile {tile} in {sessions} sessions");
    // The close: the step tree's logits leaf of that row (the seat's own, which is the accused's) beside
    // the disclosed trace tile.
    let PalwTirTraceEventDisclosureV1::Tiled { generated_token_ids, row_root, row_opening, tile_lanes, tile_opening, .. } =
        event_disclosure(&accused, row, tile)
    else {
        unreachable!("the tiled scheme")
    };
    let post = (f.space.occurrences().len() - 1) as u32;
    let position = f.ctx.declared_prefill_tokens - 1 + row;
    let leaf = f
        .leaves
        .iter()
        .position(|l| {
            matches!(l.kind, kaspa_consensus_core::palw_tir_step_v1::PalwTirLeafKindV1::Commit { occurrence, node, first_element, .. }
                if occurrence == post && node == f.space.program.logits && l.position == position
                    && (first_element as usize) <= lane && lane < first_element as usize + l.value_count as usize)
        })
        .expect("the row's logits leaf") as u64;
    let accusation = PalwTirLogitsConsistencyV1 {
        binding: accused.binding.clone(),
        step_opening: kaspa_consensus_core::palw_step_leg::step_opening_v1(&honest.hashes, leaf).expect("an opening"),
        step_preimage: honest.preimages[leaf as usize].clone(),
        trace: PalwTirTraceLanesV1::Tiled { generated_token_ids, row_root, row_opening, tile_lanes, tile_opening },
    };
    let verdict = check_tir_logits_consistency_v1(&accusation, &RULES).expect("the trace tile convicts");
    assert_eq!(verdict.fault, PalwStepFaultV1::TirLogitsTraceMismatch { value_index: lane as u32 });
    let mut one_move = palw_tir_one_move_accusation_v1(
        claim_id,
        &claim,
        bond_key(SEAT),
        PalwCourtVerdictV2::ExecutorGuilty,
        PalwCourtVerdictProofV2::TirLogits { accusation: Box::new(accusation) },
    );
    one_move.signature = vec![9; 8];
    palw_tir_one_move_shape_v1(&one_move).expect("the accusation's shape");
    let court = PalwCourtParamsV2::new(LADDER, 20, 2).expect("a court");
    assert_eq!(
        palw_tir_one_move_verdict_v1(&run.s, &claim, &one_move, &court, LADDER, PalwPromptIdsFormV1::Flat),
        Ok(PalwCourtVerdictV2::ExecutorGuilty),
        "the acceptance layer re-derives the verdict"
    );
    assert!(palw_accuser_exposure_v1(&run.s, &bond_key(SEAT)) > 0);
    run.step(&[PalwConsensusObjectV2::TirShardCourtAccused { accusation: Box::new(one_move) }]);
    assert!(
        matches!(
            run.s.claim(&claim_id).map(|c| &c.phase),
            Some(PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtFraud, .. })
        ),
        "the trace liar is voided for fraud"
    );
    assert_eq!(palw_accuser_exposure_v1(&run.s, &bond_key(SEAT)), 0, "the seat's refuted exposure is refunded");
}

/// **A rows-tree answer that does not reach the claim's trace root is refused, and a unit past the
/// trace is answered by the binding**: a tampered frontier, sibling or id, another row's answer, the
/// honest trace's answer; a row at the decode count and a node above the rows tree proven out of range.
#[test]
fn an_inconsistent_rows_answer_is_refused_and_a_row_past_the_trace_is_proven_so() {
    let f = fixture();
    let honest = f.honest();
    let decode = f.rows.len() as u64;
    let r = f.rows.len() / 2;
    let mut rows = f.rows.clone();
    rows[r][0] += 1;
    let accused = f.commit(&f.values, &rows, &f.generated);
    let (mut run, claim_id) = claimed(&f, &accused);
    let unit = PalwDaUnitV1::TirRowNode { level: 0, index: r as u64 };
    // Below the fence a rows-tree demand and its answer are refused by name (the acceptance walk's
    // drop reads the same predicate).
    let row_demand = demand(claim_id, unit, OTHER);
    assert!(palw_object_is_tir_fence2_v1(&row_demand));
    assert!(palw_object_is_tir_fence2_v1(&row_answer(&f, &accused, claim_id, 0, r as u64)));
    assert!(matches!(run.try_at(FENCE2 - 1, std::slice::from_ref(&row_demand), None), Err(PalwStateV2Error::TirFence2Refused(_))));
    assert!(matches!(
        run.try_at(FENCE2 - 1, &[row_answer(&f, &accused, claim_id, 0, r as u64)], None),
        Err(PalwStateV2Error::TirFence2Refused(_))
    ));
    run.at(FENCE2, &[row_demand], None);
    let refused = |run: &Run, object: PalwConsensusObjectV2, what: &str| {
        let e = run.refused(&[object]);
        assert!(matches!(e, PalwStateV2Error::DaOpeningRefused { .. } | PalwStateV2Error::DaAnswerMalformed { .. }), "{what}: {e}");
    };
    let tampered = |edit: &dyn Fn(&mut kaspa_consensus_core::palw_tir_court_v1::PalwTirRowNodeDisclosureV1)| {
        let PalwConsensusObjectV2::MaterialDisclosedV2 { answer: PalwDaAnswerV1::TirRowNode(mut d), .. } =
            row_answer(&f, &accused, claim_id, 0, r as u64)
        else {
            unreachable!()
        };
        edit(&mut d);
        disclosed(claim_id, unit, PalwDaAnswerV1::TirRowNode(d))
    };
    refused(&run, tampered(&|d| d.frontier[0] = h64(0xF00)), "a tile leaf flipped");
    refused(&run, tampered(&|d| d.frontier.push(h64(0xF01))), "a tile leaf added");
    refused(&run, tampered(&|d| d.siblings[0] = h64(0xF02)), "a sibling flipped");
    refused(&run, tampered(&|d| d.generated_token_ids[0] ^= 1), "an id changed");
    let PalwConsensusObjectV2::MaterialDisclosedV2 { answer: neighbour, .. } =
        row_answer(&f, &accused, claim_id, 0, (r as u64 + 1) % decode)
    else {
        unreachable!()
    };
    refused(&run, disclosed(claim_id, unit, neighbour), "another row's answer");
    refused(&run, row_answer(&f, &honest, claim_id, 0, r as u64), "the honest trace's answer");
    refused(&run, out_of_range(&accused, claim_id, unit), "an out-of-range proof of a row inside the trace");
    run.step(&[row_answer(&f, &accused, claim_id, 0, r as u64)]);
    assert!(run.session_units(&claim_id, OTHER).is_none(), "the honest answer refutes the session");
    // Past the trace: the binding proves it.
    for unit in [
        PalwDaUnitV1::TirRowNode { level: 0, index: decode },
        PalwDaUnitV1::TirRowNode { level: palw_tir_step_tree_height_v1(decode) + 1, index: 0 },
    ] {
        run.step(&[demand(claim_id, unit, OTHER)]);
        let PalwConsensusObjectV2::MaterialDisclosedV2 { answer: first_row, .. } = row_answer(&f, &accused, claim_id, 0, 0) else {
            unreachable!()
        };
        refused(&run, disclosed(claim_id, unit, first_row), "a row's answer for a unit past the trace");
        run.step(&[out_of_range(&accused, claim_id, unit)]);
        assert!(run.session_units(&claim_id, OTHER).is_none(), "{unit:?}: proven past the trace");
    }
}

// =================================================================================================
// A real-size execution: its data availability at its class's ladder
// =================================================================================================

/// The corpus class at a real-size job: 4-lane tiles over 131,072 positions, its canonical-shaped
/// binding committing past 2^22 step leaves — what every real-size class's job does (Llama-3.1-70B at
/// 512 positions and 64-lane tiles: 112 M). Nothing is executed: a binding verifies by its counts and
/// its roots, which is what an out-of-range proof carries.
fn real_size_binding(f: &Fixture) -> PalwTirStepBindingV1 {
    use kaspa_consensus_core::palw_tir_step_v1::{PALW_TIR_STEP_BINDING_VERSION_V1, PalwTirStepSpaceV1};
    let positions = 1u32 << 17;
    let mut class = f.class.clone();
    let logits = class.layout.commit_tiles.iter().position(|t| *t == 4096);
    class.layout.max_context = positions;
    for (k, tile) in class.layout.commit_tiles.iter_mut().enumerate() {
        if Some(k) != logits {
            *tile = 4;
        }
    }
    let class_id = class.class_id(&f.artifact_root);
    let mut job_context = f.ctx.clone();
    job_context.shape_profile_id = class_id;
    job_context.declared_prefill_tokens = 1;
    job_context.exact_decode_tokens = positions;
    job_context.max_context_tokens = positions + 1;
    let space = PalwTirStepSpaceV1::new(&class).expect("the layout fits");
    let count = space.leaf_count_capped(&job_context, u64::MAX).expect("a count");
    assert!(count > MAX, "a real-size execution commits past 2^22 leaves: {count}");
    let (trace, step_root) = (h64(0x7E_7E), h64(0x5E_5E));
    PalwTirStepBindingV1 {
        version: PALW_TIR_STEP_BINDING_VERSION_V1,
        committed_execution_root: palw_tir_execution_root_v1(&job_context.context_hash(), &trace, &class_id, count, &step_root),
        job_context,
        class,
        artifact_root: f.artifact_root,
        full_logits_trace_root: trace,
        step_leaf_count: count,
        step_merkle_root: step_root,
    }
}

/// **An execution past 2^22 step leaves answers its data-availability demands past the second IR
/// fence** — its binding verified at its class's ladder (the court's step ladder the held regime
/// rides, 2^40 on testnet-12), not at the release's 2^22, where every answer its producer could give
/// was refused and a demand defaulted an honest producer:
///
/// * at the class's ladder a step leaf past the execution is demanded and the claim's binding proves
///   it out of range: the session is refuted, the claim stands;
/// * at the release's 2^22 the same demand is refused at the door, and a demand inside the execution
///   admits no answer the binding can give: the out-of-range proof does not verify.
#[test]
fn an_execution_past_two_to_the_twenty_two_leaves_answers_at_its_class_s_ladder() {
    use kaspa_consensus_core::palw_tir_court_v1::check_tir_step_out_of_range_v1;
    let f = fixture();
    let binding = real_size_binding(&f);
    let count = binding.step_leaf_count;
    let (trace, exec) = (binding.full_logits_trace_root, binding.committed_execution_root);
    let past = PalwDaUnitV1::TirStepLeaf { index: count };
    assert!(check_tir_step_out_of_range_v1(trace, exec, &past, &binding, MAX).is_err(), "at 2^22 the binding does not verify");
    check_tir_step_out_of_range_v1(trace, exec, &past, &binding, 1 << 40)
        .expect("at the class's ladder it proves the leaf out of range");
    assert!(check_tir_step_out_of_range_v1(trace, exec, &PalwDaUnitV1::TirStepLeaf { index: count - 1 }, &binding, 1 << 40).is_err());

    // The claim: the real-size execution's roots, under the corpus class (its program the record's).
    let x = Execution { preimages: Vec::new(), hashes: Vec::new(), rows: Vec::new(), generated: Vec::new(), binding };
    let (mut run, claim_id) = claimed(&f, &x);
    run.extras.held_context_ladder = Some(1 << 40);
    run.at(FENCE2, &[], None);
    run.step(&[demand(claim_id, past, OTHER)]);
    assert_eq!(run.session_units(&claim_id, OTHER), Some(vec![past]), "a leaf past 2^22 is demanded at the class's ladder");
    run.step(&[out_of_range(&x, claim_id, past)]);
    assert!(run.session_units(&claim_id, OTHER).is_none(), "the binding proves it out of range: refuted");
    assert!(matches!(run.s.claim(&claim_id).expect("live").phase, PalwClaimPhaseV2::PanelBound { .. }), "the claim stands");

    // The release's 2^22: the demand past it refused at the door, one inside it unanswerable.
    let (mut run, claim_id) = claimed(&f, &x);
    run.at(FENCE2, &[], None);
    assert!(matches!(run.refused(&[demand(claim_id, past, OTHER)]), PalwStateV2Error::TirFence2Refused(_)));
    let inside = PalwDaUnitV1::TirStepLeaf { index: MAX - 1 };
    run.step(&[demand(claim_id, inside, OTHER)]);
    assert!(
        matches!(run.refused(&[out_of_range(&x, claim_id, inside)]), PalwStateV2Error::DaOpeningRefused { .. }),
        "no answer verifies at 2^22"
    );
}

/// **The widest step-node answer rides one carrier**: a frontier ten levels down (1,024 hashes), the
/// opening of a node at level ten under a 2^40-leaf root (thirty siblings), a binding with a real-size
/// class's layout and the widest network id, and an ML-DSA-87 signature — the lifecycle carrier's
/// payload inside one 100,000-byte object chunk.
#[test]
fn the_widest_step_node_answer_rides_one_carrier() {
    use kaspa_consensus_core::palw_lifecycle_objects_v2::{PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2};
    use kaspa_consensus_core::palw_state_v2::PALW_OBJECT_CHUNK_MAX_BYTES;
    let f = fixture();
    let mut binding = stripped(&real_size_binding(&f));
    binding.class.layout.commit_tiles = vec![4_096; 256];
    binding.class.layout.state_tiles = vec![4_096; 64];
    binding.job_context.network_id = vec![0x7F; kaspa_consensus_core::palw_v2::PALW_V2_MAX_NETWORK_ID_BYTES];
    let frontier = vec![h64(1); 1 << PALW_TIR_STEP_NODE_DEPTH_V1];
    let d = PalwTirStepNodeDisclosureV1 { binding, frontier, siblings: vec![h64(2); 30] };
    let object = PalwConsensusObjectV2::MaterialDisclosedV2 {
        claim: h64(3),
        unit: PalwDaUnitV1::TirStepNode { level: 40, index: 0 },
        answer: PalwDaAnswerV1::TirStepNode(Box::new(d)),
        discloser: bond_key(SEAT),
        signature: vec![0; 4_627],
    };
    let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object }).expect("serializes");
    eprintln!("the widest step-node answer's carrier payload: {} bytes of {PALW_OBJECT_CHUNK_MAX_BYTES}", payload.len());
    assert!(payload.len() <= PALW_OBJECT_CHUNK_MAX_BYTES, "{} bytes", payload.len());
}
