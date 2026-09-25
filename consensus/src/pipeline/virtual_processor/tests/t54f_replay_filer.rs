//! **ADR-0152 v3.1 Phase 2, P2-8b / P2-8d on real claims — T54f's processor half**: the replay filer's
//! ONE builder (`palw_replay_refute_v1::palw_replay_contradiction_v1`, which kaspad's filer runs off its
//! loop) on claims the producer built, and every object it leads to taken the whole way — the gate,
//! the acceptance walk and the fold.
//!
//! A child of T46's suite, so the harness, its doors and its assertions are that suite's own: the
//! claim is the template this node builds and the producer's own run (a garbage trace is
//! `execute_with_injected_fault`'s self-consistent lie; garbage logits are `Fault::GarbageLogits`'
//! bent row), the seat's LOCAL replay is the floor's `execute` of the claim's job, and the SERVED
//! capture is the claim's own committed one.
//!
//! What each test proves:
//! * **T54f** — the replaying seat bisects the served garbage trace against its own replay to the
//!   divergent step in at most `1 + ⌈log₂ n⌉` reserved rungs; the builder's `ExecutorRefuted` is the
//!   very object the suite convicts with, and it takes P2-8's reporter-filer road (commit, root, file,
//!   reveal — P2-8's `Filing`, `commit_then_file` and `reveal_and_paid`) and folds with S2's charge;
//!   the filer's refuted session is refunded (V3S-06: net ≥ 0) and its reward R arrives after the
//!   reveal. Its fence-off twin (`palw_rcore_plus` off) folds the same bytes
//!   with the pre-R-core charge — and kaspad's filer never runs there (`palw_replay_filer_armed_v1`).
//! * **F1c** — garbage logits on an honest step tree: the producer's capture fails every seat's rule
//!   (SEAT-0's head rule is the lie) yet carries the claim's binding, and the builder files
//!   `LogitsNotStepOutput` (12), the suite's own contradiction, which folds with S2.
//! * **P2-8d** — the divergent leaf is located but the served material cannot open it: the seat's
//!   `StepLeaf` demand is asked of the fold's gates for THAT unit (`palw_da_step_leaf_demand_check_v1`
//!   — never P2-6's row-0 read, which tells the seat that already accused row 0 `AccusedBefore`),
//!   opens the J-6 garbage path's second session of the SAME seat, and the silent producer defaults:
//!   S1, the `DaDefault` record, every accuser uncharged; the producer that answers it from its capture
//!   hands the fold the guilty leaf, and the claim voids `CourtFraud`.
//! * **Liveness** — an honest claim yields nothing, and a seat whose own replay is WRONG never
//!   convicts an honest producer: the proof it builds from the served capture holds.
//!
//! kaspad's half — the book, the budgets, the ledger, the door to the reporter filer and the carrier
//! lane — is
//! `kaspad::palw_panel::palw_filer_replay`'s tests; a node e2e on a devnet preset is a POST-LAUNCH
//! item (the operator moved every drill and node launch after the t12 launch, 2026-09-24).
use super::*;
use kaspa_consensus_core::palw_backend::{PalwClaimRootsV1, PalwMaterialVerdictV1};
use kaspa_consensus_core::palw_da_rcore_v1::{PalwDaAnswerV1, PalwDaUnitV1, palw_da_offence_id_v1};
use kaspa_consensus_core::palw_held_da_v1::PalwHeldMissingV1;
use kaspa_consensus_core::palw_producer_v2::PalwDaAccusationCheckV1 as Check;
use kaspa_consensus_core::palw_producer_v2::PalwDaStepLeafDemandCheckV1 as Demand;
use kaspa_consensus_core::palw_replay_refute_v1::{
    PalwReplayClaimV1, PalwReplayFindingV1, PalwReplayNothingV1, PalwReplaySiteV1, palw_replay_bisect_rungs_v1,
    palw_replay_contradiction_v1, palw_replay_executor_refuted_object_v1,
};
use kaspa_consensus_core::palw_state_v2::palw_accuser_exposure_v1;

use super::t46_p2_8_reporter_filer::{Filing, capture_arm_fault, commit_then_file, reveal_and_paid};

/// The roots every seat arm checks a served capture against — the claim's, the block's anchor and
/// draw rule — as kaspad's filer builds them from its seat duty.
fn roots_of(h: &H, claim: &RealClaim) -> PalwClaimRootsV1 {
    let attempt = &claim.envelope.attempt;
    PalwClaimRootsV1 {
        execution_root: attempt.execution_root,
        trace_root: attempt.trace_root,
        anchor: claim.anchor,
        attempt_draw: Some(h.config.params.palw_prefill_draw_active_at(claim.header.daa_score)),
        output_root: Some(attempt.output_root),
        job_pin: None,
    }
}

/// **The replaying seat's run**: its own dense replay of the claim's job against `served`, through
/// `backend`, with the target resolved from state as the gate resolves it. Returns the finding and
/// the rungs the builder reserved.
fn replay(
    h: &H,
    walk: &Walk,
    claim: &RealClaim,
    backend: &dyn PalwExecutionBackendV1,
    served: &[u8],
    local: &[u8],
) -> (PalwReplayFindingV1, u32) {
    let target = kaspa_consensus_core::palw_offence_attribution_v1::palw_offence_target_v1(&walk.state, &claim.claim_id)
        .expect("the claim is in state");
    let n = backend.capture_shape(served).expect("a capture").step_leaf_count;
    let mut reserved = 0u32;
    let finding = palw_replay_contradiction_v1(
        backend,
        served,
        local,
        PalwReplayClaimV1 {
            target: &target,
            roots: roots_of(h, claim),
            ladder: h.ladder(&walk.state),
            form: h.form(),
            prompt_token_ids: None,
        },
        palw_replay_bisect_rungs_v1(n),
        |_rung| {
            reserved += 1;
            Ok::<(), String>(())
        },
    );
    (finding, reserved)
}

/// This seat's own replay of the claim's job: the floor's run, the job the anchor implies.
fn local_run(h: &H, claim: &RealClaim) -> Vec<u8> {
    h.backend.execute(&claim.job, &claim.prompt).expect("the seat replays the claim's job").material
}

/// The node's read of card `card`'s `StepLeaf` demand of `leaf` at the next block — the fold's gates
/// for the named unit, through the processor's own extras.
fn demand_check(h: &H, walk: &Walk, claim: Hash64, card: usize, leaf: u64) -> Demand {
    let point = walk.next();
    h.vp().palw_da_step_leaf_demand_check_v1_at(&walk.state, point.block, point.daa_score, &claim, &h.cards[card], leaf)
}

/// Card `card`'s event accusation of row 0 — the ONE builder P2-6's seats file with.
fn accuse_row_0(h: &H, claim: Hash64, card: usize) -> Obj {
    kaspa_consensus_core::palw_da_rcore_v1::palw_da_accusation_object_v1(
        &h.domain,
        claim,
        kaspa_consensus_core::palw_da_rcore_v1::PALW_DA_AUTO_NAMED_UNIT_V1,
        h.cards[card],
        |message, context| Some(sign(card, message, context)),
    )
    .expect("built")
}

/// The producer answers row 0 from its own capture — the ONE builder P2-7's responder files with (the
/// garbage strategy answers DA: its capture is self-consistent).
fn producer_answers_row_0(h: &H, claim: &RealClaim) -> Obj {
    let disclosure = h.backend.disclose_trace_event(&claim.material, 0, 0).expect("the capture opens row 0");
    kaspa_consensus_core::palw_da_rcore_v1::palw_da_answer_object_v1(
        &h.domain,
        claim.claim_id,
        PalwDaUnitV1::Event { row: 0, tile: 0 },
        PalwDaAnswerV1::Event(disclosure),
        h.cards[EXECUTOR],
        h.bundle.court.max_close_bytes(),
        |message, context| Some(sign(EXECUTOR, message, context)),
    )
    .expect("built")
}

/// **T54f: a garbage trace, bisected by the replaying seat to its divergent step, refutes its executor
/// — S2 — and the filer's refuted session is refunded (V3S-06).**
///
/// A real floor claim whose committed step tree holds the drill's one-lane lie, bound to cards 1–5.
/// A seat of the panel first accuses row 0 (P2-6's builder) and the garbage producer answers it from
/// its self-consistent capture (P2-7's builder): the session is refuted and its exposure HELD (DA-6).
/// The seat's own replay then disagrees with the claim's roots; the builder bisects the served capture
/// against it to the injected leaf in at most `1 + ⌈log₂ n⌉` rungs — each reserved — and returns
/// `StepArithmetic`, which the fold's own kind-4 adjudicator convicts. The object it encodes is byte
/// for byte the one this suite convicts with (`H::refuted`), and it takes the whole road: the claim
/// voids `CourtFraud`, the executor pays S2 (the forfeit and `min(10% · C, 3 G)`), and the seat's held
/// exposure returns in the conviction's block, so its net is 0 — plus R: the node hands the filing to
/// P2-8's reporter filer, so the seat committed before the evidence folded, reveals after it, and the
/// reward arrives at its payout when the window closes.
#[tokio::test]
async fn t54f_a_garbage_trace_is_bisected_to_its_step_and_its_executor_refuted_s2() {
    let h = harness(true);
    let mut walk = h.genesis_walk();
    let claim = h.open_claim(&mut walk, Fault::Step);
    let id = claim.claim_id;
    let leaf = claim.fault_leaf.expect("the lie's leaf");
    h.bind(&mut walk, id);
    assert!(h.sp().rcore_plus_active_at(walk.daa), "testnet-12 arms R-core+ from genesis");
    let seat = PANEL[0];
    let collateral = walk.state.bond(&h.cards[seat]).unwrap().collateral;
    // The J-6 garbage path's first round: the seat's session, answered by the producer — refuted, held.
    h.carry(&mut walk, vec![accuse_row_0(&h, id, seat)]);
    h.carry(&mut walk, vec![producer_answers_row_0(&h, &claim)]);
    assert!(walk.state.da_session(&id, &h.cards[seat]).is_none(), "every unit answered: refuted");
    let held = palw_accuser_exposure_v1(&walk.state, &h.cards[seat]);
    assert!(held > 0 && walk.state.da_claim(&id).unwrap().refuted_held == vec![(h.cards[seat], held)], "held (DA-6)");
    // The seat's replay, bisected against the served capture.
    let n = h.backend.capture_shape(&claim.material).unwrap().step_leaf_count;
    let (finding, reserved) = replay(&h, &walk, &claim, &h.backend, &claim.material, &local_run(&h, &claim));
    let PalwReplayFindingV1::Refutes { contradiction, prompt_ids_opening, site, rungs } = finding else {
        panic!("the garbage trace is refuted: {finding:?}")
    };
    assert_eq!(site, PalwReplaySiteV1::StepLeaf(leaf), "the first divergent step is the lie's");
    assert_eq!(reserved, rungs, "every rung was reserved before it ran");
    assert!(rungs <= palw_replay_bisect_rungs_v1(n), "{rungs} rungs over {n} leaves: 1 + ⌈log₂ n⌉ at most");
    assert!(rungs >= 2, "a real bisection, not the whole-space rung alone");
    h.judge_refuted(&walk.state, id, contradiction.clone()).expect("kind 4's adjudicator convicts it");
    let built = palw_replay_executor_refuted_object_v1(id, h.cards[EXECUTOR], contradiction, prompt_ids_opening).expect("built");
    assert_eq!(built.object, h.refuted(id, claim.contradiction()), "the node files the very proof this suite convicts with");
    assert_eq!(
        built.offence_id,
        kaspa_consensus_core::palw_offence_attribution_v1::palw_executor_refuted_offence_id_v1(&h.cards[EXECUTOR].0, &id)
    );
    // The reporter filer's road (R-3): keyed as the chain keys it, committed, rooted, filed.
    let (offence_id, evidence_id) = (built.offence_id, built.evidence_id);
    let filing = Filing::of(built.object, [0x8B; 32]);
    assert_eq!((filing.key, filing.evidence_id), (offence_id, evidence_id), "the filer keys it as the chain does");
    let (before, committed_at) = commit_then_file(&h, &mut walk, &filing, seat);
    assert_refuted_before_final(&h, &before, &walk.state, id, walk.daa, claim.envelope.attempt.execution_root);
    // V3S-06: the filer's refuted cost comes back in the conviction's block.
    assert!(walk.state.da_claim(&id).unwrap().refuted_held.is_empty(), "refunded (DA-6)");
    assert_eq!(palw_accuser_exposure_v1(&walk.state, &h.cards[seat]), 0);
    assert_eq!(walk.state.bond(&h.cards[seat]).unwrap().collateral, collateral, "the filer nets 0 — never less");
    // ... plus R, after the reveal.
    let paid = reveal_and_paid(&h, &mut walk, std::slice::from_ref(&filing), seat, committed_at);
    assert!(paid[0] > 0, "the replaying seat is paid for the conviction it filed");
}

/// **One offence found by two lanes is ONE filing, one conviction and one reward** (the integration of
/// P2-8 with P2-8b). On one lying claim the capture arm samples the lie's leaf (P2-8's `FaultAt`) and
/// the replaying seat bisects the served capture to the same leaf (P2-8b): both lanes build
/// `ExecutorRefuted` under the claim's ONE kind-4 key and over the same bytes — the key the node's
/// reporter filer holds one filing under, whichever lane hands it first (kaspad's
/// `one_offence_found_by_the_capture_arm_and_a_replay_is_filed_once` pins the book's half). The
/// filing the node makes — the capture arm's, which runs first in a tick — commits, roots, folds and
/// is revealed. The replay's, asked of the chain after, reads its key consumed: the filer answers it
/// `AlreadyConvicted` and it is never handed; a copy carried anyway is refused at the gate, dropped
/// by the walk and folds as a no-op. The claim is charged once, and the one reward is the seat's.
#[tokio::test]
async fn t54f_one_offence_found_by_the_capture_arm_and_a_replay_is_filed_once() {
    let h = harness(true);
    let mut walk = h.genesis_walk();
    let claim = h.open_claim(&mut walk, Fault::Step);
    let id = claim.claim_id;
    let leaf = claim.fault_leaf.expect("the lie's leaf");
    h.bind(&mut walk, id);
    let seat = PANEL[0];
    // The capture arm's FaultAt at the lie's leaf, and the replaying seat's bisection of the claim.
    let sampled = Filing::of(capture_arm_fault(&h, &walk, &claim, leaf), [0x5E; 32]);
    let (finding, _) = replay(&h, &walk, &claim, &h.backend, &claim.material, &local_run(&h, &claim));
    let PalwReplayFindingV1::Refutes { contradiction, prompt_ids_opening, site, .. } = finding else { panic!("{finding:?}") };
    assert_eq!(site, PalwReplaySiteV1::StepLeaf(leaf), "both lanes find the lie's leaf");
    let built = palw_replay_executor_refuted_object_v1(id, h.cards[EXECUTOR], contradiction, prompt_ids_opening).expect("built");
    let replayed = Filing::of(built.object, [0x8B; 32]);
    assert_eq!(replayed.key, sampled.key, "one kind-4 key a claim, whichever lane found it");
    assert_eq!((replayed.evidence_id, &replayed.object), (sampled.evidence_id, &sampled.object), "the same proof, byte for byte");
    // The one filing: committed, rooted, folded.
    let (before, committed_at) = commit_then_file(&h, &mut walk, &sampled, seat);
    assert_refuted_before_final(&h, &before, &walk.state, id, walk.daa, claim.envelope.attempt.execution_root);
    // The replay's filing, asked of the chain now: consumed — never handed; a copy carried anyway is nothing.
    let read = replayed.read(&h, &walk, seat, true);
    assert!(read.consumed.is_some_and(|record| record.accepted_daa == walk.daa), "the filer's AlreadyConvicted");
    let point = walk.next();
    let empty = h.fold(&walk.state, &point, &[]).expect("an empty block folds").state_root();
    h.validate(&walk.state, &point, &replayed.object).expect_err("the gate refuses a second proof of one offence");
    assert!(h.accepted(&walk.state, &point, std::slice::from_ref(&replayed.object)).is_empty(), "the walk drops it");
    let folded = h.fold(&walk.state, &point, std::slice::from_ref(&replayed.object)).expect("the fold carries it as a no-op");
    assert_eq!(folded.state_root(), empty, "a second copy changes nothing");
    // One reward, the seat's.
    let paid = reveal_and_paid(&h, &mut walk, std::slice::from_ref(&sampled), seat, committed_at);
    assert!(paid.len() == 1 && paid[0] > 0);
}

/// **T54f's fence-off twin: below `palw_rcore_plus` the same builder finds the same step and its
/// object folds as it always did** — the pre-R-core charge (the forfeit alone), through the same
/// gate, walk and fold. kaspad's filer never runs there (`palw_replay_filer_armed_v1`); what this pins
/// is that nothing the builder produces moves the fold below the fence.
#[tokio::test]
async fn t54f_twin_below_rcore_plus_the_same_proof_folds_unchanged() {
    let h = harness_rcore_off();
    let mut walk = h.genesis_walk();
    let claim = h.open_claim(&mut walk, Fault::Step);
    let id = claim.claim_id;
    h.bind(&mut walk, id);
    assert!(!h.sp().rcore_plus_active_at(walk.daa), "the twin's bundle has R-core+ off");
    let (finding, _) = replay(&h, &walk, &claim, &h.backend, &claim.material, &local_run(&h, &claim));
    let PalwReplayFindingV1::Refutes { contradiction, prompt_ids_opening, site, .. } = finding else { panic!("{finding:?}") };
    assert_eq!(site, PalwReplaySiteV1::StepLeaf(claim.fault_leaf.unwrap()));
    let built = palw_replay_executor_refuted_object_v1(id, h.cards[EXECUTOR], contradiction, prompt_ids_opening).unwrap();
    assert_eq!(built.object, h.refuted(id, claim.contradiction()));
    let (before, _) = h.carry(&mut walk, vec![built.object]);
    assert_refuted_before_final(&h, &before, &walk.state, id, walk.daa, claim.envelope.attempt.execution_root);
}

/// **F1c on a replay: garbage logits over an honest step tree are refuted by `LogitsNotStepOutput`.**
/// The producer commits the honest step tree beside one bent logits lane (its argmax untouched) and
/// serves the capture that commitment is ([`bent_capture`]) — which NO seat verifies: SEAT-0's head
/// rule refuses committed rows that are not the head's step outputs, which is the lie itself. The
/// capture still carries the claim's own binding, so the builder takes F1c's route alone (no bisection,
/// no demand on bytes no seat verified): it finds the bent lane against its own replay and files 12 at
/// that head tile — the suite's own contradiction, authenticated by the fold's predicate against the
/// claim's roots — and it folds with S2 (forfeit by root).
#[tokio::test]
async fn t54f_f1c_garbage_logits_on_an_honest_step_tree_are_refuted_by_12() {
    let h = harness(true);
    let mut walk = h.genesis_walk();
    let claim = h.open_claim(&mut walk, Fault::GarbageLogits);
    let id = claim.claim_id;
    h.bind(&mut walk, id);
    let served = bent_capture(&claim);
    assert_eq!(
        h.backend.verify_material(&served, roots_of(&h, &claim)),
        PalwMaterialVerdictV1::Mismatch,
        "SEAT-0's head rule refuses the lie at every seat"
    );
    let (finding, reserved) = replay(&h, &walk, &claim, &h.backend, &served, &local_run(&h, &claim));
    let PalwReplayFindingV1::Refutes { contradiction, prompt_ids_opening, site, rungs } = finding else { panic!("{finding:?}") };
    assert_eq!((rungs, reserved), (0, 0), "no bisection on bytes no seat verified");
    let (row, lane) = bent_lane(&claim);
    let head = kaspa_consensus_core::palw_step::palw_logits_head_v1(&claim.binding.shape_profile).unwrap();
    assert_eq!(site, PalwReplaySiteV1::LogitsHead { row, head_tile: lane / head.tile_len });
    assert_eq!(contradiction, h.logits_not_step_output(&claim, row, lane), "the suite's own 12");
    assert!(prompt_ids_opening.is_none());
    let built = palw_replay_executor_refuted_object_v1(id, h.cards[EXECUTOR], contradiction, None).unwrap();
    let (before, _) = h.carry(&mut walk, vec![built.object]);
    assert_refuted_before_final(&h, &before, &walk.state, id, walk.daa, claim.envelope.attempt.execution_root);
    h.reloads(&walk.state);
}

/// **The capture a `GarbageLogits` producer serves**: its honest step tiles and checkpoint chunks, and
/// the bent rows under the bent binding it committed — the floor's retained tuple
/// (`base0_material_encode_v1`'s layout) re-encoded, so it verifies against the claim's own roots.
fn bent_capture(claim: &RealClaim) -> Vec<u8> {
    let (_, tiles, _, ids, chunks) = misaka_palw_base0::produce::base0_material_decode_v1(&claim.material).expect("decodes");
    let rows = &claim.pin.as_ref().expect("the committed rows").logits_rows;
    borsh::to_vec(&(&claim.binding, &tiles, rows, &ids, &chunks)).expect("encodes")
}

/// **The floor, as a seat that holds the claim's capture but cannot open a leaf of it** — every verb
/// the builder reads is the floor's, but the leaf prover refuses (the trait's default): the node's view
/// of served material that verifies against the claim and does not carry the divergent leaf's proof.
struct LeafOpeningsWithheld<'a>(&'a Base0Backend);

impl PalwExecutionBackendV1 for LeafOpeningsWithheld<'_> {
    fn model_id(&self) -> &str {
        self.0.model_id()
    }
    fn job_for_anchor(&self, anchor: Hash64) -> Result<(PalwJobContextV2, Vec<usize>), String> {
        self.0.job_for_anchor(anchor)
    }
    fn execute(
        &self,
        job: &PalwJobContextV2,
        prompt: &[usize],
    ) -> Result<kaspa_consensus_core::palw_backend::PalwExecutionOutcomeV1, String> {
        self.0.execute(job, prompt)
    }
    fn verify_material(&self, material: &[u8], claim: PalwClaimRootsV1) -> PalwMaterialVerdictV1 {
        self.0.verify_material(material, claim)
    }
    fn capture_shape(&self, material: &[u8]) -> Option<kaspa_consensus_core::palw_backend::PalwCaptureShapeV1> {
        self.0.capture_shape(material)
    }
    fn disclose_trace_event(
        &self,
        material: &[u8],
        row: u32,
        tile: u8,
    ) -> Result<kaspa_consensus_core::palw_step_refute::PalwTraceEventDisclosureV1, String> {
        self.0.disclose_trace_event(material, row, tile)
    }
    fn bisect_prefix_state(&self, material: &[u8], index: u64) -> Option<Hash64> {
        self.0.bisect_prefix_state(material, index)
    }
}

/// **The J-6 garbage path up to the demand, on the SAME seat** (the P2-8b review's finding 2): a real
/// garbage-trace claim, bound; seat PANEL[0] accuses row 0 (P2-6's builder) and, while that session
/// is open, the demand read says `SessionOpen` (wait: the fold refuses a second session of one
/// accuser); the garbage producer answers row 0 from its self-consistent capture (P2-7's builder) and
/// the session closes refuted. P2-6's read now says `AccusedBefore` for that seat — the rule that
/// dropped this demand — while the demand read, the fold's own gates for the named leaf, says
/// `File` for it as a seat, and `File` for a bystander as a non-seat (DA-3). The builder locates
/// the lie's leaf on the verified capture and, the leaf's proof unopenable from what was served,
/// demands it. Returns the walk, the claim, the leaf, the seat and the demand's binding.
fn j6_up_to_the_demand(h: &H) -> (Walk, RealClaim, u64, usize, PalwStepBindingV2) {
    let mut walk = h.genesis_walk();
    let claim = h.open_claim(&mut walk, Fault::Step);
    let id = claim.claim_id;
    let leaf = claim.fault_leaf.unwrap();
    h.bind(&mut walk, id);
    let seat = PANEL[0];
    h.carry(&mut walk, vec![accuse_row_0(h, id, seat)]);
    assert_eq!(demand_check(h, &walk, id, seat, leaf), Demand::SessionOpen, "its row-0 session is open: wait");
    h.carry(&mut walk, vec![producer_answers_row_0(h, &claim)]);
    assert!(walk.state.da_session(&id, &h.cards[seat]).is_none(), "the row-0 session is refuted and closed");
    let point = walk.next();
    assert_eq!(
        h.vp().palw_da_accusation_check_v1_at(&walk.state, point.block, point.daa_score, &id, &h.cards[seat]),
        Check::AccusedBefore,
        "P2-6's read: once per accuser — the rule that settled this demand before the fix"
    );
    let Demand::File { admission } = demand_check(h, &walk, id, seat, leaf) else {
        panic!("the fold admits the same seat's second session: {:?}", demand_check(h, &walk, id, seat, leaf))
    };
    assert!(admission.accuser_is_seat, "a seat's session (DA-8: four a claim)");
    let Demand::File { admission } = demand_check(h, &walk, id, BYSTANDER, leaf) else { panic!("a non-seat files too (DA-3)") };
    assert!(!admission.accuser_is_seat);
    let withheld = LeafOpeningsWithheld(&h.backend);
    let (finding, _) = replay(h, &walk, &claim, &withheld, &claim.material, &local_run(h, &claim));
    let PalwReplayFindingV1::DemandLeaf { leaf: named, binding, .. } = finding else { panic!("a demand: {finding:?}") };
    assert_eq!((named, &*binding), (leaf, &claim.binding), "the lie's leaf, under the claim's own binding");
    (walk, claim, leaf, seat, *binding)
}

/// The seat's `StepLeaf` demand, as the ONE held builder signs it with the seat's card.
fn demand_object(
    h: &H,
    claim: &RealClaim,
    card: usize,
    leaf: u64,
    binding: &PalwStepBindingV2,
) -> Result<Obj, kaspa_consensus_core::palw_da_rcore_v1::PalwDaHeldAccusationBuildErrorV1> {
    kaspa_consensus_core::palw_da_rcore_v1::palw_da_held_accusation_object_v1(
        &h.domain,
        claim.claim_id,
        &claim.envelope.attempt.execution_root,
        PalwHeldMissingV1::StepLeaf { leaf },
        binding.clone(),
        h.cards[card],
        h.form(),
        |message, context| Some(sign(card, message, context)),
    )
}

/// **P2-8d (T54f's demand half, DA-3, J-6): a divergent leaf the served material cannot prove is
/// demanded by the same seat that accused row 0, and a silent producer defaults.**
///
/// [`j6_up_to_the_demand`], then: a unit the binding does not commit is refused by the builder before
/// any carrier; the demand clears the gate, the walk and the fold and opens the seat's SECOND session,
/// whose named unit is `StepLeaf { leaf }` (free past `palw_rcore_plus`, beside the fold's drawn
/// width-1 ranges) and which pauses the claim; the read then says `SessionOpen`. The producer discloses
/// nothing: the first block past the deadline voids the claim `ProducerWithholding`, takes S1 (the
/// whole commitment) and records the `DaDefault`; the seat's exposure returns and its refuted row-0
/// exposure is refunded — nobody but the producer pays.
#[tokio::test]
async fn t54f_p2_8d_an_unprovable_divergent_leaf_is_demanded_and_a_silent_producer_defaults() {
    let h = harness(true);
    let (mut walk, claim, leaf, seat, binding) = j6_up_to_the_demand(&h);
    let id = claim.claim_id;
    assert!(
        matches!(
            demand_object(&h, &claim, seat, claim.binding.step_leaf_count, &binding),
            Err(kaspa_consensus_core::palw_da_rcore_v1::PalwDaHeldAccusationBuildErrorV1::Refused(_))
        ),
        "a leaf the binding does not commit is refused before a carrier"
    );
    let producer = h.cards[EXECUTOR];
    let producer_before = walk.state.bond(&producer).unwrap().collateral;
    let commitment =
        kaspa_consensus_core::palw_state_v2::palw_claim_bond_reservation_v1(h.sp(), walk.state.claim(&id).unwrap()).unwrap();
    let collateral = walk.state.bond(&h.cards[seat]).unwrap().collateral;
    h.carry(&mut walk, vec![demand_object(&h, &claim, seat, leaf, &binding).expect("the ONE held builder builds it")]);
    let session = walk.state.da_session(&id, &h.cards[seat]).expect("the demand opened the seat's second session").clone();
    assert_eq!(session.units[0], PalwDaUnitV1::Held(PalwHeldMissingV1::StepLeaf { leaf }), "the named leaf (DA-3)");
    assert!(session.accuser_is_seat, "a seat's session");
    assert_eq!(walk.state.da_claim(&id).unwrap().opened_by_seat.get(&h.cards[seat]), Some(&2), "the same seat's second");
    assert_eq!(walk.state.deadline_of(&id), None, "it pauses the claim (DA-5)");
    assert_eq!(demand_check(&h, &walk, id, seat, leaf), Demand::SessionOpen, "its demand is on chain: nothing more to file");
    h.reloads(&walk.state);
    // The producer is silent: the first block past the deadline.
    let point = walk.at(session.deadline_daa + 1);
    let next = h.fold(&walk.state, &point, &[]).expect("the sweep's block folds");
    walk.advance(&point, next);
    assert!(
        matches!(walk.state.claim(&id).unwrap().phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::ProducerWithholding, .. }),
        "the silent producer defaults: {:?}",
        walk.state.claim(&id).unwrap().phase
    );
    assert_eq!(u128::from(producer_before - walk.state.bond(&producer).unwrap().collateral), commitment, "S1: the whole commitment");
    let record = walk.state.consumed_offence(&palw_da_offence_id_v1(&producer.0, &id)).expect("the DaDefault");
    assert_eq!(record.kind, PalwOffenceKindV1::DaDefault);
    assert_eq!(palw_accuser_exposure_v1(&walk.state, &h.cards[seat]), 0, "the seat's exposure is returned");
    assert_eq!(walk.state.bond(&h.cards[seat]).unwrap().collateral, collateral, "the seat pays nothing");
    assert!(
        !matches!(demand_check(&h, &walk, id, seat, leaf), Demand::File { .. }),
        "a voided claim is demanded no more: {:?}",
        demand_check(&h, &walk, id, seat, leaf)
    );
    h.reloads(&walk.state);
}

/// **J-6 completed by the fold: the producer that answers the demand hands it the guilty leaf.** The
/// same seat's demand as above; the garbage producer answers the named `StepLeaf` from its own capture
/// (P2-7's ONE held builder — the honest-looking evidence of its lie); the fold adjudicates it and the
/// verdict convicts: the claim voids `CourtFraud`, the producer is charged, the seat pays nothing (its
/// sessions released, its refuted row-0 exposure returned). No further filing is needed after a demand
/// is answered, and the read then settles it (the leaf answered, or the claim no longer accusable).
#[tokio::test]
async fn t54f_p2_8d_a_demand_the_producer_answers_convicts_it_court_fraud() {
    use kaspa_consensus_core::palw_da_rcore_v1::{
        palw_da_answer_object_v1, palw_da_held_answer_v1, palw_da_held_disclosure_from_capture_v1,
    };
    let h = harness(true);
    let (mut walk, claim, leaf, seat, binding) = j6_up_to_the_demand(&h);
    let id = claim.claim_id;
    let producer = h.cards[EXECUTOR];
    let producer_before = walk.state.bond(&producer).unwrap().collateral;
    let collateral = walk.state.bond(&h.cards[seat]).unwrap().collateral;
    h.carry(&mut walk, vec![demand_object(&h, &claim, seat, leaf, &binding).expect("built")]);
    assert!(walk.state.da_session(&id, &h.cards[seat]).is_some(), "the demand's session");
    // The producer answers the named leaf from its capture, as P2-7's responder does on the attempt lane.
    let record = walk.state.claim(&id).unwrap().clone();
    let prompt: Vec<u32> = claim.prompt.iter().map(|t| u32::try_from(*t).expect("a u32 id")).collect();
    let roots = PalwClaimRootsV1 { output_root: None, ..roots_of(&h, &claim) };
    let missing = PalwHeldMissingV1::StepLeaf { leaf };
    let (answer_binding, disclosure) = palw_da_held_disclosure_from_capture_v1(
        &h.backend,
        &claim.material,
        &prompt,
        roots,
        record.work_leaves,
        missing,
        h.form(),
        || h.backend.disclose_trace_event(&claim.material, u32::MAX, u8::MAX).map(|d| d.binding().clone()),
    )
    .expect("the producer's capture opens the leaf it lied at");
    let answer = palw_da_answer_object_v1(
        &h.domain,
        id,
        PalwDaUnitV1::Held(missing),
        palw_da_held_answer_v1(id, missing, answer_binding, disclosure),
        producer,
        h.bundle.court.max_close_bytes(),
        |message, context| Some(sign(EXECUTOR, message, context)),
    )
    .expect("the ONE answer builder builds it");
    h.carry(&mut walk, vec![answer]);
    assert!(
        matches!(walk.state.claim(&id).unwrap().phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtFraud, .. }),
        "the guilty leaf convicts: {:?}",
        walk.state.claim(&id).unwrap().phase
    );
    assert!(walk.state.bond(&producer).unwrap().collateral < producer_before, "the producer is charged");
    assert_eq!(palw_accuser_exposure_v1(&walk.state, &h.cards[seat]), 0, "the seat's exposure is returned");
    assert!(walk.state.bond(&h.cards[seat]).unwrap().collateral >= collateral, "the seat nets >= 0 (V3S-06)");
    assert!(
        matches!(demand_check(&h, &walk, id, seat, leaf), Demand::Answered | Demand::Refused(_)),
        "settled: {:?}",
        demand_check(&h, &walk, id, seat, leaf)
    );
    h.reloads(&walk.state);
}

/// **Liveness: an honest claim yields nothing, and a WRONG replay never convicts an honest producer.**
/// On an honest claim the seat's replay reproduces the roots: nothing, no rung spent. A seat whose own
/// replay is faulty — the drill's lie in ITS run — bisects to its own divergent leaf, opens the proof
/// from the served (honest) capture, and the fold's predicate holds it: `NotConvicted`, never a filing
/// and never a demand. A served capture that is not the claim's is not the committed execution.
#[tokio::test]
async fn t54f_an_honest_claim_files_nothing_whatever_the_seats_replay() {
    let h = harness(true);
    let mut walk = h.genesis_walk();
    let claim = h.open_claim(&mut walk, Fault::Honest);
    h.bind(&mut walk, claim.claim_id);
    let honest_local = local_run(&h, &claim);
    let (finding, reserved) = replay(&h, &walk, &claim, &h.backend, &claim.material, &honest_local);
    assert_eq!((finding, reserved), (PalwReplayFindingV1::Nothing { why: PalwReplayNothingV1::LocalReproduces, rungs: 0 }, 0));
    let n = claim.binding.step_leaf_count;
    let wrong_at = (n / 2..n).find(|leaf| h.backend.refutation_for_index(&claim.material, *leaf).is_ok()).expect("an openable leaf");
    let wrong_local =
        h.backend.execute_with_injected_fault(&claim.job, &claim.prompt, wrong_at).expect("the faulty seat's own run").material;
    let (finding, _) = replay(&h, &walk, &claim, &h.backend, &claim.material, &wrong_local);
    let PalwReplayFindingV1::Nothing { why: PalwReplayNothingV1::NotConvicted { site, why }, .. } = finding else {
        panic!("a wrong replay files nothing against an honest producer: {finding:?}")
    };
    assert_eq!(site, PalwReplaySiteV1::StepLeaf(wrong_at), "it bisected to its own lie, and the served step holds: {why}");
    let (finding, _) = replay(&h, &walk, &claim, &h.backend, &wrong_local, &honest_local);
    assert_eq!(finding, PalwReplayFindingV1::Nothing { why: PalwReplayNothingV1::ServedNotTheClaims, rungs: 0 });
}
