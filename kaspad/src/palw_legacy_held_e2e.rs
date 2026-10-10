//! **LG14-B — the legacy V2 route under G14, on the held 8k fixture, through the real fold, with a FRESH outsider.**
//!
//! The producer AND both seats of the claim's panel collude: both sign `Valid` and the claim is licensed. One bonded verifier
//! OUTSIDE the panel (`OUTSIDER`, registered as an ordinary bond, never a seat or the producer) prosecutes it from public material
//! only — the objects on the chain, the claim's public job, and ITS OWN honest replica of the job on its own copy of the registered
//! model (ADR-0177: G14 is conditional on that copy). It never reads the producer's instance, its served capture or a seat's copy:
//! there is no `ServedView` here. What the PRODUCER answers comes from its own instance (the liar's re-derives its lie) — that is its
//! obligation, not the verifier's material.
//!
//! The fold is the shipped transition (`apply_palw_transition_v2_with_extras`), with R-core+ in force and
//! `palw_legacy_held_da_v2` TEST-ARMED through the extras flag (the fence itself is refused on every network). Every block's delta
//! re-applies and reverts, and its carriage reloads under its root. Level: **V-fold** (G14C's counting rules) — the processor's
//! gate and walk for tags 157–159 are pinned by `consensus/.../t12_legacy_held_da_gate.rs`; a chain-path (mempool → template) run
//! waits on G14C's canonical harness.
//!
//! * **Gap (b) and (c), the fused lie** ([`lg14b_a_lie_in_one_fused_tile_is_localized_and_convicted_by_a_fresh_outsider`]): row 0
//!   answers (the binding becomes public), the descent lands on the lie's own fused leaf from authenticated frontiers alone, the
//!   committed tile is disclosed by `KernelWitness`, the outsider's own kernel says it is not the attention of its history, the
//!   held dissection opens on its `ShardCourtAccused` and its held route plays the liar to `CourtHeldVerdict`.
//! * **Gap (a)**: the outsider's replica is a FOLD read node by node — a handful of replayed blocks, never the whole capture.
//! * **The consistent-garbage shape at the embedding gather and a mid-layer matmul** — the leaf recompute (tag 159), before and after
//!   `Final`, from openings the outsider builds itself.
//! * **Withholding** — a descent node or a witness unanswered: the DA default (`ProducerWithholding`), never a fraud verdict.
//! * **An honest claim and a malicious outsider** — every answer authenticates, the session is refuted, a recompute of an honest leaf
//!   is a false accusation the outsider pays, and the claim reaches `Final`.
//! * **Below the fence** every legacy held object is refused by the fold and the state is untouched.
use super::*;
use crate::palw_legacy_held_v2::{
    PalwLegacyTreeV2, palw_legacy_fused_opening_v2, palw_legacy_held_answer_object_v2, palw_legacy_held_answer_v2,
    palw_legacy_held_demand_object_v2, palw_legacy_leaf_recompute_object_v2, palw_legacy_leaf_recompute_v2,
};
use kaspa_consensus_core::palw_da_rcore_v1::{PalwDaAnswerV1, PalwDaUnitV1};
use kaspa_consensus_core::palw_legacy_held_da_v2::{
    PalwLegacyDescentStepV2, PalwLegacyFrontierV2, PalwLegacyHeldAnswerV2, PalwLegacyHeldUnitV2, PalwLegacyNodeViewV2,
    PalwLegacyTreeV2 as Tree, palw_legacy_descent_next_v2, palw_legacy_descent_rounds_v2, palw_legacy_leaf_recompute_verdict_v2,
    palw_legacy_verifier_resident_bytes_v2,
};
use kaspa_consensus_core::palw_shard_court_v1::{PalwOneMoveClaimV2, PalwShardCourtVerdictV1};
use kaspa_consensus_core::palw_state_v2::PalwStateV2Error;
use kaspa_consensus_core::palw_step_leg::PalwStepBindingV2;
use std::collections::HashSet;

/// The ordinary bond outside the claim's panel: neither the producer nor a seat.
const OUTSIDER: u64 = 6;
const SIG: [u8; 8] = [0x6B; 8];

// ---- the fold: R-core+ in force, the fence test-armed -----------------------------------------------------------------------------

fn rc_params() -> PalwStateParamsV2 {
    params().with_rcore_plus_mirrors(Some(0), 0, Vec::new())
}

fn rc_extras(armed: bool) -> PalwTransitionExtrasV1 {
    PalwTransitionExtrasV1 { legacy_held_da_v2_active: armed, ..launch() }
}

/// One block through the real transition, checked as T54g's are: consistency, the delta re-applies and reverts, the carriage reloads.
fn rc_step_with(
    parent: &PalwChainStateV2,
    daa: u64,
    objects: &[PalwConsensusObjectV2],
    att: Option<&PalwAttemptEnvelopeV2>,
    armed: bool,
) -> Result<PalwChainStateV2, PalwStateV2Error> {
    let p = rc_params();
    let (child, delta) =
        apply_palw_transition_v2_with_extras(parent, &p, &point(daa), objects, att, false, false, false, true, &rc_extras(armed))?;
    child.assert_internal_consistency(&p).expect("internal consistency");
    child.assert_deadline_consistency(&p).expect("deadline consistency");
    assert_eq!(apply_delta_v2(parent, &delta, &p).expect("re-applies"), child, "DAA {daa}: the delta is the transition");
    assert_eq!(revert_delta_v2(&child, &delta, &p).expect("reverts"), *parent, "DAA {daa}: the delta reverts");
    let reloaded = PalwStateCarriageV2::from_state(&child).into_state(&p, Some(child.state_root())).expect("reloads");
    assert_eq!(reloaded, child, "DAA {daa}: reload is the state");
    Ok(child)
}

fn rc_step(parent: &PalwChainStateV2, daa: u64, objects: &[PalwConsensusObjectV2]) -> Result<PalwChainStateV2, PalwStateV2Error> {
    rc_step_with(parent, daa, objects, None, true)
}

/// **The producer's run of the fixture's free-prompt job**, honest or lying in ONE committed tile at `leaf` (the drill: its first
/// lane moved, every other leaf the honest execution's), the fold retained — [`produce`]'s recipe at a leaf the case chooses.
fn produce_at(
    artifact: &Arc<Base0ArtifactV1>,
    profile: &PalwShapeProfileV3,
    lie: Option<&dyn Fn(&PalwJobContextV2) -> u64>,
) -> Produced {
    use kaspa_consensus_core::palw_backend::PalwFreePromptDrillFaultV1;
    let honest = produce(artifact, profile, false);
    let Some(pick) = lie else { return honest };
    let leaf = pick(&honest.ctx);
    let prompt: Vec<usize> = honest.ids.iter().map(|t| *t as usize).collect();
    let run = honest
        .backend
        .execute_free_prompt_with_drill_fault_v2(&honest.fp_job, &prompt, PalwFreePromptDrillFaultV1::Leaf { leaf })
        .expect("the liar's run commits");
    let engine = misaka_palw_base0::engine_a16::A16Engine::new(artifact).expect("an A16 class");
    let plan = engine.plan_from_profile(profile).expect("the class's plan");
    let dense = misaka_palw_base0::qwen25_a16_backend::a16_execute_in_storage_v1(
        artifact,
        profile,
        Some(&plan),
        &honest.ctx,
        &prompt,
        LADDER,
        misaka_palw_base0::legs::Base0CaptureKindV1::DenseTiles,
        &mut |_| {},
        Some(leaf),
        misaka_palw_base0::engine_a16::KV_STORAGE_SHIPPED_V1,
    )
    .expect("the dense re-derivation");
    let committed =
        misaka_palw_base0::produce::base0_material_decode_any_v1(&run.outcome.material).expect("decodes").binding().clone();
    assert_eq!(
        kaspa_consensus_core::palw_step_leg::step_merkle_root_capped_v1(&dense.tiles.leaves, LADDER).expect("roots"),
        committed.step_merkle_root,
        "the re-derived leaves are the fold's committed tree"
    );
    Produced {
        material: run.outcome.material,
        execution_root: run.outcome.execution_root,
        trace_root: run.outcome.trace_root,
        leaf,
        committed_leaves: dense.tiles.leaves,
        ..honest
    }
}

/// The class's first non-fused matmul of layer `layer` at `position`, tile 0.
fn matmul_leaf_at(profile: &PalwShapeProfileV3, job: &PalwJobContextV2, layer: u16, position: u32) -> u64 {
    let index = profile.attn_nodes.iter().position(|n| n.op_kind == PalwStepOpKindV1::MatMulQuant).expect("a matmul");
    let slot = profile.global_node_slot(PalwStepTableV1::Attn, layer, index).expect("the slot");
    let coord = PalwStepCoordinateV1 { call_index: 0, node_slot: slot, position, tile_index: 0 };
    canonical_step_leaf_index(profile, job, &coord).expect("the matmul's leaf")
}

/// **The claim on the chain, its panel bound and licensed by both colluding seats** — [`bound`]'s registry (plus `OUTSIDER`, an
/// ordinary bond) and attempt, under R-core+, then the licence. Returns the licensed state at 103 and the claim.
fn rc_licensed(
    d: &Produced,
    canonical: &PalwJobContextV2,
    profile: &PalwShapeProfileV3,
    artifact_root: Hash64,
) -> (PalwChainStateV2, Hash64) {
    let class_id = profile.shape_profile_id();
    let bond = |n: u64| PalwConsensusObjectV2::BondRegistered {
        bond: bond_key(n),
        pubkey: vec![n as u8; 4],
        operator_pubkey: op_key(20 + n),
        // Deep enough that each seat's R-core+ lock fits its work room (the backed subset licenses).
        collateral: 1 << 50,
        payout_payload: Hash64::from_u64_word(0x9A00 + n),
        capable_classes: Default::default(),
        signature: Vec::new(),
    };
    let register = vec![
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
        bond(SEAT),
        bond(COLLUDER),
        bond(OUTSIDER),
        PalwConsensusObjectV2::ClassRegistered {
            class_id,
            artifact_root,
            slash_value_per_pwu: 5,
            pwu_rule: PalwPwuRuleV2::MaxPerAttempt(160),
            initial_target: u128::MAX / 2,
            share_permille: 100,
            activation_daa: 0,
            admission: Some(Box::new(PalwClassAdmissionCarriageV2 {
                profile: profile.clone(),
                canonical: canonical.clone(),
                registrant_bond: bond_key(PRODUCER),
                signature: Vec::new(),
            })),
        },
    ];
    let s = rc_step(&PalwChainStateV2::genesis(), 100, &register).expect("the registry");
    let network_domain = h64(999);
    let executor_bond = bond_key(PRODUCER).0;
    let env = PalwAttemptEnvelopeV2 {
        attempt: PalwAttemptUnsignedV2 {
            version: PALW_ATTEMPT_V2_VERSION,
            network_domain,
            challenge: challenge_v2(network_domain, h64(5), 1_700, 1, h64(1), &executor_bond),
            class_id,
            executor_bond,
            executor_pubkey: vec![7; 4],
            operator_id: palw_operator_id_v2(&op_key(20 + PRODUCER)),
            artifact_root,
            trace_root: d.trace_root,
            output_root: h64(32),
            pwu: 40,
            trace_manifest_root: h64(33),
            trace_chunk_count: 4,
            trace_retention_daa: 999_999,
            execution_root: d.execution_root,
        },
        signature: vec![0; 8],
    };
    let claim = attempt_id_v2(&env.attempt);
    let s = rc_step_with(&s, 101, &[], Some(&env), true).expect("the claim");
    let seats = vec![
        PalwPanelSeatV2 { bond: bond_key(SEAT), operator_id: palw_operator_id_v2(&op_key(20 + SEAT)) },
        PalwPanelSeatV2 { bond: bond_key(COLLUDER), operator_id: palw_operator_id_v2(&op_key(20 + COLLUDER)) },
    ];
    let s = rc_step(&s, 102, &[PalwConsensusObjectV2::PanelBound { claim, anchor: h64(77), seats }]).expect("the panel");
    let valid = |seat: u64| PalwSeatReceiptV2 {
        claim: Hash64::default(),
        verdict: PalwReceiptVerdictV2::Valid,
        seat_bond: bond_key(seat),
        signed_daa: 0,
        signature: Vec::new(),
    };
    let s = rc_step(&s, 103, &[PalwConsensusObjectV2::ReceiptLicensed { claim, receipts: vec![valid(SEAT), valid(COLLUDER)] }])
        .expect("both seats license the lie");
    assert!(
        matches!(phase_of(&s, &claim), PalwClaimPhaseV2::ReceiptLicensed { .. }),
        "licensed by every seat: {:?}",
        phase_of(&s, &claim)
    );
    (s, claim)
}

// ---- the chain as public material -------------------------------------------------------------------------------------------------

/// **The world**: the tip, the next DAA, and every object the chain carried — the outsider's only window onto the claim.
struct World {
    s: PalwChainStateV2,
    daa: u64,
    chain: Vec<PalwConsensusObjectV2>,
}

impl World {
    /// One block carrying `objects`; the objects join the public record.
    fn block(&mut self, objects: Vec<PalwConsensusObjectV2>) -> Result<(), PalwStateV2Error> {
        self.s = rc_step(&self.s, self.daa, &objects)?;
        self.daa += 1;
        self.chain.extend(objects);
        Ok(())
    }

    fn quiet(&mut self) {
        self.block(Vec::new()).expect("an empty block folds");
    }
}

/// The claim's binding as the chain discloses it: the event answer a row-0 demand obtained.
fn public_binding(chain: &[PalwConsensusObjectV2], claim: &Hash64) -> Option<PalwStepBindingV2> {
    chain.iter().find_map(|o| match o {
        PalwConsensusObjectV2::MaterialDisclosedV2 { claim: c, answer: PalwDaAnswerV1::Event(disclosure), .. } if c == claim => {
            Some(disclosure.binding().clone())
        }
        _ => None,
    })
}

/// The answered frontiers of `tree` the chain holds for the claim, in the form the descent reads.
fn public_frontiers(chain: &[PalwConsensusObjectV2], claim: &Hash64, tree: Tree, leaf_count: u64) -> Vec<PalwLegacyFrontierV2> {
    chain
        .iter()
        .filter_map(|o| match o {
            PalwConsensusObjectV2::LegacyHeldAnsweredV2 { answer } if answer.claim == *claim => {
                match (answer.unit.node(), &answer.answer) {
                    (Some((t, level, index)), PalwLegacyHeldAnswerV2::Node { frontier, .. }) if t == tree => {
                        PalwLegacyFrontierV2::of_answer(leaf_count, level, index, frontier)
                    }
                    _ => None,
                }
            }
            _ => None,
        })
        .collect()
}

/// The committed witness the chain holds for the claim's leaf.
fn public_witness(
    chain: &[PalwConsensusObjectV2],
    claim: &Hash64,
    leaf: u64,
) -> Option<kaspa_consensus_core::palw_legacy_held_da_v2::PalwCommittedKernelWitnessV2> {
    chain.iter().find_map(|o| match o {
        PalwConsensusObjectV2::LegacyHeldAnsweredV2 { answer }
            if answer.claim == *claim && answer.unit == PalwLegacyHeldUnitV2::KernelWitness { leaf } =>
        {
            match &answer.answer {
                PalwLegacyHeldAnswerV2::KernelWitness(witness) => Some((**witness).clone()),
                _ => None,
            }
        }
        _ => None,
    })
}

/// **The producer's obligation**: its answer to `unit`, from its own instance and its committed leaves (a liar's re-derive its lie).
fn producer_answers(d: &Produced, binding: &PalwStepBindingV2, claim: Hash64, unit: PalwLegacyHeldUnitV2) -> PalwConsensusObjectV2 {
    let step_tree = PalwLegacyTreeV2::leaves_v1(d.committed_leaves.clone());
    let checkpoint_tree = PalwLegacyTreeV2::leaves_v1(checkpoint_leaf_hashes(&d.material));
    let answer = palw_legacy_held_answer_v2(
        &unit,
        &step_tree,
        &checkpoint_tree,
        &d.backend,
        &d.material,
        &d.ids,
        d.roots(),
        d.backend.prompt_ids_form(),
    )
    .expect("the producer answers from its retention");
    palw_legacy_held_answer_object_v2(&h64(999), claim, unit, binding.clone(), answer, bond_key(PRODUCER), |_, _| Some(SIG.to_vec()))
        .expect("built")
}

/// The checkpoint leaf hashes a fold's binding commits, from the leaves it retains.
fn checkpoint_leaf_hashes(material: &[u8]) -> Vec<Hash64> {
    let m = misaka_palw_base0::produce::base0_fp_material_decode_v2(material).expect("a fold");
    let (context_hash, _, checkpoint_profile_hash) =
        kaspa_consensus_core::palw_step_leg::verify_binding_v1(&m.binding).expect("binds");
    m.checkpoint_leaves
        .iter()
        .map(|leaf| {
            kaspa_consensus_core::palw_step_leg::checkpoint_leaf_hash_v2(
                &context_hash,
                &checkpoint_profile_hash,
                &m.binding.state_chunk_map_id,
                leaf,
            )
        })
        .collect()
}

// ---- the outsider -----------------------------------------------------------------------------------------------------------------

/// **The verifier outside the panel**: a fresh honest instance on its own copy of the registered model, the claim's public job, and
/// its own replica of that job (a fold).
struct Outsider {
    backend: Qwen25A16Backend,
    capture: Vec<u8>,
    roots: PalwClaimRootsV1,
    ids: Vec<u32>,
}

impl Outsider {
    /// Started AFTER the claim: it runs the public job itself.
    fn start(f: &Fixture, d: &Produced) -> Self {
        let backend = f.seat();
        let prompt: Vec<usize> = d.ids.iter().map(|t| *t as usize).collect();
        let run = backend.execute_free_prompt(&d.fp_job, &prompt).expect("the outsider's own replica");
        let roots = PalwClaimRootsV1 {
            execution_root: run.outcome.execution_root,
            trace_root: run.outcome.trace_root,
            anchor: kaspa_consensus_core::palw_freeprompt_v3::fp_job_id_v3(&d.fp_job),
            attempt_draw: None,
            output_root: None,
            job_pin: None,
        };
        Self { backend, capture: run.outcome.material, roots, ids: d.ids.clone() }
    }

    fn tree(&self) -> PalwLegacyTreeV2<'_> {
        PalwLegacyTreeV2::fold_v1(&self.backend, &self.capture, &self.ids).expect("its replica is a fold")
    }

    fn demand(&self, claim: Hash64, unit: PalwLegacyHeldUnitV2, binding: &PalwStepBindingV2) -> PalwConsensusObjectV2 {
        palw_legacy_held_demand_object_v2(&h64(999), claim, unit, bond_key(OUTSIDER), binding.clone(), |_, _| Some(SIG.to_vec()))
            .expect("built")
    }
}

/// What the localization reached, and what it cost.
struct Located {
    binding: PalwStepBindingV2,
    leaf: u64,
    rounds: u32,
    replays: u64,
    resident_leaves: u64,
}

/// **The public localization**, block by block: row 0 (the binding), then the descent — each demand the outsider files from the
/// chain's frontiers and its own replica, each answer the producer owes (or, `withhold`, none). Returns `Err(unit)` with the unit left
/// unanswered when the producer withholds it.
fn localize(w: &mut World, d: &Produced, o: &Outsider, claim: Hash64, withhold: bool) -> Result<Located, PalwLegacyHeldUnitV2> {
    // Row 0: an event demand any bond may file; the producer's answer makes its binding public.
    w.block(vec![PalwConsensusObjectV2::DefaultAccused {
        claim,
        missing_event_index: 0,
        accuser: bond_key(OUTSIDER),
        signature: SIG.to_vec(),
    }])
    .expect("a non-seat's row-0 demand opens a session");
    let disclosure = d.backend.disclose_trace_event(&d.material, 0, 0).expect("row 0");
    w.block(vec![PalwConsensusObjectV2::MaterialDisclosedV2 {
        claim,
        unit: PalwDaUnitV1::Event { row: 0, tile: 0 },
        answer: PalwDaAnswerV1::Event(disclosure),
        discloser: bond_key(PRODUCER),
        signature: SIG.to_vec(),
    }])
    .expect("row 0 answers: a consistent trace opens");
    let binding = public_binding(&w.chain, &claim).expect("the binding is public");
    let n = binding.step_leaf_count;
    let own = o.tree();
    let mut rounds = 0u32;
    let leaf = loop {
        let frontiers = public_frontiers(&w.chain, &claim, Tree::Step, n);
        match palw_legacy_descent_next_v2(Tree::Step, n, &binding.step_merkle_root, &own, &frontiers) {
            PalwLegacyDescentStepV2::Demand(unit) => {
                rounds += 1;
                w.block(vec![o.demand(claim, unit, &binding)]).expect("the descent's demand opens a session");
                if withhold {
                    return Err(unit);
                }
                w.block(vec![producer_answers(d, &binding, claim, unit)]).expect("the fold authenticates the producer's frontier");
            }
            PalwLegacyDescentStepV2::FirstDivergentLeaf(leaf) => break leaf,
            other => panic!("the descent ended {other:?}"),
        }
    };
    let (replays, resident_leaves) = own.resources();
    Ok(Located { binding, leaf, rounds, replays, resident_leaves })
}

fn bonds_collateral(s: &PalwChainStateV2) -> (u64, u64) {
    (collateral(s, PRODUCER), collateral(s, OUTSIDER))
}

// ---- the outsider's held route (the A-held line's, as a non-seat challenger) -------------------------------------------------------

/// The held route's host for the outsider: T54g's [`HeldHost`] with the outsider's bond, and the claims it may still pursue read as a
/// non-seat reads them (disputable, with no date).
struct OutsiderHost {
    inner: HeldHost,
}

impl PalwHeldHostV1 for OutsiderHost {
    fn offence_attribution_active(&self, daa: u64) -> bool {
        self.inner.offence_attribution_active(daa)
    }
    fn class_is_held(&self, class_id: &Hash64) -> Option<bool> {
        self.inner.class_is_held(class_id)
    }
    fn opening_cap(&self, class_id: &Hash64, daa: u64) -> u64 {
        self.inner.opening_cap(class_id, daa)
    }
    fn arity(&self, daa: u64) -> Option<u8> {
        self.inner.arity(daa)
    }
    fn disclose_window_daa(&self) -> u64 {
        self.inner.disclose_window_daa()
    }
    fn window_court(&self) -> u64 {
        self.inner.window_court()
    }
    fn network_domain(&self) -> Hash64 {
        self.inner.network_domain()
    }
    fn bond(&self) -> PalwBondKeyV2 {
        bond_key(OUTSIDER)
    }
    fn verdict_of(&self, session_id: &Hash64, proof: &PalwCourtVerdictProofV2) -> Option<PalwCourtVerdictV2> {
        self.inner.verdict_of(session_id, proof)
    }
    fn sign(&self, message: &[u8], context: &[u8]) -> Option<Vec<u8>> {
        self.inner.sign(message, context)
    }
    fn pin(&self, claim: Hash64) {
        self.inner.pin(claim)
    }
    fn ledger(&self) -> Arc<PalwMemoryLedgerV1> {
        self.inner.ledger()
    }
    fn backend(&self, duty: &PalwCourtDutyV2) -> Result<Box<dyn PalwExecutionBackendV1>, String> {
        self.inner.backend(duty)
    }
    fn build_need_bytes(&self, backend: &dyn PalwExecutionBackendV1, duty: &PalwCourtDutyV2) -> u64 {
        self.inner.build_need_bytes(backend, duty)
    }
    fn responder_material(&self, duty: &PalwCourtDutyV2, backend: &dyn PalwExecutionBackendV1) -> Result<PalwHeldMaterialV1, String> {
        self.inner.responder_material(duty, backend)
    }
    fn carried_prompt(&self, duty: &PalwCourtDutyV2, backend: &dyn PalwExecutionBackendV1) -> Result<Option<Vec<u32>>, String> {
        self.inner.carried_prompt(duty, backend)
    }
    fn claim_open_until_v1(&self, claim_id: &Hash64) -> Option<u64> {
        let disputable = kaspa_consensus_core::palw_producer_v2::palw_disputable_claims_v2(&self.inner.state, &[bond_key(OUTSIDER)]);
        crate::palw_panel::held_court::palw_held_open_claims_v1(&[], &disputable, &HashSet::from([*claim_id])).get(claim_id).copied()
    }
}

/// **The outsider's held route for one tick** — [`held_tick`]'s order with the outsider's bond.
async fn outsider_tick(
    host: &mut OutsiderHost,
    held: &mut PalwHeldCourtV1,
    queue: &mut Queue,
    state: &PalwChainStateV2,
    chain: &[PalwConsensusObjectV2],
    daa: u64,
) -> Option<PalwConsensusObjectV2> {
    host.inner.state = state.clone();
    let host: &OutsiderHost = host;
    let duties = palw_court_duties_v2(state, &[bond_key(OUTSIDER)]);
    held.begin_tick_v1(&duties, daa).await;
    let held_duties: Vec<PalwCourtDutyV2> = duties.into_iter().filter(|d| held.routes_v1(host, d, daa)).collect();
    for read in held.chain_reads_v1(host, &held_duties, daa) {
        let objects = chain
            .iter()
            .filter(|o| crate::palw_panel::palw_held_chain_object_is_the_sessions_v1(o, &read.session_id, &read.claim_id))
            .cloned()
            .collect();
        held.note_chain_v1(read.session_id, objects, daa);
    }
    let (pending, moved) = (&queue.pending, &queue.moved);
    let busy = |key: &(Hash64, u32, bool)| {
        pending.iter().any(|(a, b, c, _)| (*a, *b, *c) == *key)
            || moved.get(key).is_some_and(|at| daa < at.saturating_add(crate::palw_panel::COURT_MOVE_REPLAN_DAA))
    };
    let tick = palw_held_moves_v1(host, held, &held_duties, daa, busy);
    for queued in tick.queued {
        queue.due.insert(queued.key, queued.due);
        queue.pending.push((queued.key.0, queued.key.1, queued.key.2, queued.object));
    }
    palw_held_start_builds_v1(host, held, daa);
    held.settle_v1().await;
    queue.carry(daa)
}

// ---- the cases --------------------------------------------------------------------------------------------------------------------

/// **Gaps (a), (b), (c) closed for a fresh outsider: a lie in ONE committed fused tile of the 8k row** (the shipped drill) is
/// localized from public frontiers alone, its committed tile disclosed by `KernelWitness`, and the held dissection the outsider opens
/// convicts the producer (`CourtHeldVerdict`) — while both seats signed `Valid`.
#[tokio::test(flavor = "multi_thread")]
async fn lg14b_a_lie_in_one_fused_tile_is_localized_and_convicted_by_a_fresh_outsider() {
    let f = Fixture::new(true);
    let liar = &f.d;
    let (s, claim) = rc_licensed(liar, &f.canonical, &f.profile, f.root);
    let before = bonds_collateral(&s);
    let mut w = World { s, daa: 104, chain: Vec::new() };
    let o = Outsider::start(&f, liar);
    assert_ne!(o.roots.execution_root, liar.execution_root, "the outsider's replica is honest: its roots are not the lie's");
    // 1. Localization from public material: row 0 answers, the descent lands on the lie's own fused leaf.
    let located = localize(&mut w, liar, &o, claim, false).expect("answered");
    assert_eq!(located.leaf, liar.leaf, "the first divergent leaf is the lie's own");
    assert!(kaspa_consensus_core::palw_shard_court_v1::palw_leaf_is_fused_v2(&located.binding, located.leaf), "a fused site");
    let n = located.binding.step_leaf_count;
    assert!(located.rounds >= 1 && located.rounds <= palw_legacy_descent_rounds_v2(n), "{} rounds over {n} leaves", located.rounds);
    // Gap (a): the replica was read a block at a time — never the whole capture.
    let retain = misaka_palw_base0::produce::base0_fp_material_decode_v2(&o.capture).expect("a fold").step_tree.retain_level();
    eprintln!(
        "LG14B resources: n {n} leaves, height {}, retain level {retain}, {} node rounds (bound {}), {} blocks replayed, {} leaf hashes \
         resident at most (whole capture {n})",
        kaspa_consensus_core::palw_tir_court_v1::palw_tir_step_tree_height_v1(n),
        located.rounds,
        palw_legacy_descent_rounds_v2(n),
        located.replays,
        located.resident_leaves,
    );
    assert!(located.resident_leaves < n, "resident {} of {n} leaves", located.resident_leaves);
    // 2. Gap (c): the committed fused tile, disclosed by the unit DA-3 used to refuse.
    let ckw = PalwLegacyHeldUnitV2::KernelWitness { leaf: located.leaf };
    w.block(vec![o.demand(claim, ckw, &located.binding)]).expect("a fused leaf's witness is demandable");
    w.block(vec![producer_answers(liar, &located.binding, claim, ckw)]).expect("the witness authenticates by membership alone");
    let witness = public_witness(&w.chain, &claim, located.leaf).expect("public");
    assert!(witness.refutation.inputs.is_empty() && witness.refutation.kv_checkpoint.is_none(), "no history: the dissection's");
    assert!(w.s.da_sessions_of(&claim).next().is_none(), "every session answered and closed");
    // 3. The outsider's own kernel over its own history says the tile lies; it opens the held dissection.
    let evidence = palw_legacy_fused_opening_v2(&o.backend, &o.capture, &o.ids, &witness, f.root, PALW_HELD_STEP_LADDER_V1)
        .expect("the outsider's N1")
        .expect("the committed tile is not the attention of its history");
    let bound_to =
        PalwOneMoveClaimV2 { execution_root: liar.execution_root, class_id: f.profile.shape_profile_id(), artifact_root: f.root };
    assert_eq!(evidence.verdict_at_v2(&bound_to, PALW_HELD_STEP_LADDER_V1, true), Ok(PalwShardCourtVerdictV1::NeedsDissection));
    let mut accusation =
        evidence.into_accusation_v1(claim, liar.execution_root, liar.trace_root, bond_key(PRODUCER), bond_key(OUTSIDER));
    accusation.validate_shape(PALW_HELD_STEP_LADDER_V1).expect("the shape");
    accusation.signature = SIG.to_vec();
    let opening = PalwConsensusObjectV2::ShardCourtAccused { accusation: Box::new(accusation) };
    w.block(vec![opening]).expect("the fold opens the held dissection on the outsider's accusation");
    let sid =
        w.s.court_sessions_iter()
            .find(|(_, x)| x.claim == claim && x.challenger_bond == bond_key(OUTSIDER))
            .map(|(k, _)| *k)
            .expect("the outsider's session");
    // 4. Played: the liar's least lie against the outsider's held route.
    let accused =
        liar.backend.attn_site_evidence_held_v1(&liar.material, liar.leaf, Some(&liar.ids), None).expect("the liar's own evidence");
    let site = accused.site_v1(f.root, false, PALW_HELD_STEP_LADDER_V1).expect("site");
    let (lying_root, lane, delta) = least_lie_root(&accused, f.root);
    let (a2, p2) = (f.artifact.clone(), f.profile.clone());
    let mut host = OutsiderHost {
        inner: HeldHost {
            state: w.s.clone(),
            make: Box::new(move || backend(&a2, &p2)),
            ledger: PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Host, None, || None),
            carried: o.ids.clone(),
        },
    };
    let mut held = PalwHeldCourtV1::default();
    let mut queue = Queue::default();
    let mut outsider_moves = Vec::new();
    for _ in 0..2_000 {
        if w.s.court_session(&sid).is_none() {
            break;
        }
        let mut objects = Vec::new();
        if let Some(duty) = duty_of(&w.s, PRODUCER, sid) {
            match palw_held_move_of_duty_v1(&duty) {
                Some(PalwHeldMoveV1::Root) => {
                    let mut root_object = accused.root_claim_held_v1(&site, sid, 2).expect("the liar's held root claim");
                    let PalwConsensusObjectV2::CourtAttnRootClaimedHeld { root, signature, .. } = &mut root_object else {
                        unreachable!()
                    };
                    *root = lying_root.clone();
                    *signature = vec![0xAA; 8];
                    objects.push(root_object);
                }
                Some(PalwHeldMoveV1::Round) => {
                    let phase = duty.dissection.as_ref().expect("the phase");
                    let mut round = accused.round_v1(&site, phase).expect("the liar's round");
                    let first = phase.child_ranges().iter().position(|&(f, _)| f == 0).expect("a child holds tile 0");
                    round.children[first].v_acc[lane] += delta;
                    objects.push(PalwConsensusObjectV2::CourtAttnDissected { session_id: sid, round, signature: vec![0xAA; 8] });
                }
                _ => {}
            }
        }
        if objects.is_empty()
            && let Some(object) = outsider_tick(&mut host, &mut held, &mut queue, &w.s, &w.chain, w.daa).await
        {
            outsider_moves.push(object.clone());
            objects.push(object);
        }
        let daa = w.daa;
        w.block(objects).unwrap_or_else(|e| panic!("DAA {daa}: the fold refuses a move: {e}"));
    }
    assert!(w.s.court_session(&sid).is_none(), "the held dissection closed");
    assert!(
        outsider_moves
            .iter()
            .any(|o| matches!(o, PalwConsensusObjectV2::CourtClosed { verdict: PalwCourtVerdictV2::ExecutorGuilty, .. })),
        "the outsider filed the bottom"
    );
    let PalwClaimPhaseV2::Voided { reason, .. } = phase_of(&w.s, &claim) else { panic!("voided: {:?}", phase_of(&w.s, &claim)) };
    assert_eq!(reason, PalwVoidReasonV2::CourtHeldVerdict, "a held dissection's verdict");
    let after = bonds_collateral(&w.s);
    assert!(after.0 < before.0, "the producer is charged");
    assert_eq!(after.1, before.1, "the outsider pays nothing: its refuted sessions' exposure is refunded on the conviction");
}

/// **The consistent-garbage shape: the trace diverges at leaf 0, the position-0 embedding gather** — whose committed tile, for an
/// honest producer, would be a copy of the registered weights, so no unit may compel it (ADR-0177 D2). The outsider convicts it by the
/// leaf recompute (tag 159) from the committed leaf HASH and its own model rows — before `Final`, and on a second claim after it.
#[tokio::test(flavor = "multi_thread")]
async fn lg14b_a_garbage_gather_is_convicted_by_the_leaf_recompute_before_and_after_final() {
    let f = Fixture::new(false);
    let gather = |_: &PalwJobContextV2| 0u64;
    let liar = produce_at(&f.artifact, &f.profile, Some(&gather));
    assert_eq!(liar.leaf, 0);
    for after_final in [false, true] {
        let (s, claim) = rc_licensed(&liar, &f.canonical, &f.profile, f.root);
        let before = bonds_collateral(&s);
        let mut w = World { s, daa: 104, chain: Vec::new() };
        let o = Outsider::start(&f, &liar);
        let located = localize(&mut w, &liar, &o, claim, false).expect("answered");
        assert_eq!(located.leaf, 0, "the garbage starts at the gather");
        let ckw = PalwLegacyHeldUnitV2::KernelWitness { leaf: 0 };
        assert!(
            matches!(
                rc_step(&w.s, w.daa, &[o.demand(claim, ckw, &located.binding)]),
                Err(PalwStateV2Error::LegacyHeldDaRefused { .. })
            ),
            "the gather's tile is never demanded"
        );
        if after_final {
            for _ in 0..2_000 {
                if matches!(phase_of(&w.s, &claim), PalwClaimPhaseV2::Final { .. }) {
                    break;
                }
                w.quiet();
            }
            assert!(
                matches!(phase_of(&w.s, &claim), PalwClaimPhaseV2::Final { .. }),
                "the claim went Final: no non-seat session holds it"
            );
        }
        file_the_recompute(&mut w, &f, &liar, &o, &located, claim);
        let PalwClaimPhaseV2::Voided { reason, .. } = phase_of(&w.s, &claim) else { panic!("voided: {:?}", phase_of(&w.s, &claim)) };
        assert_eq!(reason, PalwVoidReasonV2::CourtFraud, "after_final {after_final}");
        let after = bonds_collateral(&w.s);
        assert!(after.0 < before.0, "the producer is charged (after_final {after_final})");
        assert_eq!(after.1, before.1, "the outsider pays nothing (after_final {after_final})");
    }
}

/// The outsider's tag 159 at the located leaf: built from its own replica and the public frontiers, asked of the court's own verdict
/// locally, then folded.
fn file_the_recompute(w: &mut World, f: &Fixture, liar: &Produced, o: &Outsider, located: &Located, claim: Hash64) {
    let own = o.tree();
    let frontiers = public_frontiers(&w.chain, &claim, Tree::Step, located.binding.step_leaf_count);
    let committed = frontiers.iter().find_map(|fr| fr.leaf_hash(located.leaf)).expect("the bottom round carried the leaf hash");
    let view = PalwLegacyNodeViewV2 {
        leaf_count: located.binding.step_leaf_count,
        divergent: located.leaf,
        frontiers: &frontiers,
        own: &own,
    };
    let accusation = palw_legacy_leaf_recompute_v2(
        &o.backend,
        &o.capture,
        &o.ids,
        o.roots,
        o.backend.prompt_ids_form(),
        &located.binding,
        &view,
        committed,
        located.leaf,
        claim,
        liar.trace_root,
        bond_key(PRODUCER),
        bond_key(OUTSIDER),
    )
    .expect("the outsider builds the recompute");
    let bound_to =
        PalwOneMoveClaimV2 { execution_root: liar.execution_root, class_id: f.profile.shape_profile_id(), artifact_root: f.root };
    assert_eq!(
        palw_legacy_leaf_recompute_verdict_v2(&accusation, &bound_to, PALW_HELD_STEP_LADDER_V1),
        Ok(PalwShardCourtVerdictV1::ExecutorGuilty),
        "the court's own verdict, asked before filing"
    );
    let object = palw_legacy_leaf_recompute_object_v2(&h64(999), accusation, |_, _| Some(SIG.to_vec())).expect("signed");
    w.block(vec![object]).expect("the fold convicts on the recompute");
}

/// **A lie in one mid-layer matmul tile** (layer 1, a prefill position): the first divergent leaf's inputs are earlier leaves, which
/// the outsider opens in the CLAIM's tree from its own nodes and the revealed frontiers — no producer material but hashes.
#[tokio::test(flavor = "multi_thread")]
async fn lg14b_a_mid_layer_matmul_lie_is_convicted_from_openings_the_outsider_builds() {
    let f = Fixture::new(false);
    let layer = 1.min(f.profile.layer_count - 1);
    let pick = |ctx: &PalwJobContextV2| matmul_leaf_at(&f.profile, ctx, layer, LIE_POSITION);
    let liar = produce_at(&f.artifact, &f.profile, Some(&pick));
    let (s, claim) = rc_licensed(&liar, &f.canonical, &f.profile, f.root);
    let before = bonds_collateral(&s);
    let mut w = World { s, daa: 104, chain: Vec::new() };
    let o = Outsider::start(&f, &liar);
    let located = localize(&mut w, &liar, &o, claim, false).expect("answered");
    assert_eq!(located.leaf, liar.leaf);
    assert!(!kaspa_consensus_core::palw_shard_court_v1::palw_leaf_is_fused_v2(&located.binding, located.leaf));
    // ADR-0177 D2 past the fence (DA16b §7.5 item 1): the int-12 held `StepLeaf` answer owes its claim part only — with the
    // producer's model rows it is refused; without them it is accepted by membership and convicts nobody by itself.
    use kaspa_consensus_core::palw_held_da_v1::{
        PALW_HELD_DA_VERSION_V1, PalwHeldDisclosureCarriageV1, PalwHeldDisclosureV1, PalwHeldMissingV1,
    };
    let missing = PalwHeldMissingV1::StepLeaf { leaf: located.leaf };
    let demand = kaspa_consensus_core::palw_da_rcore_v1::palw_da_held_accusation_object_v1(
        &h64(999),
        claim,
        &liar.execution_root,
        missing,
        located.binding.clone(),
        bond_key(OUTSIDER),
        liar.backend.prompt_ids_form(),
        |_, _| Some(SIG.to_vec()),
    )
    .expect("a non-fused leaf is demandable");
    w.block(vec![demand]).expect("the StepLeaf demand opens a session");
    let evidence = kaspa_consensus_core::palw_leaf_evidence_v1::palw_leaf_evidence_from_capture_v1(
        &liar.backend,
        &liar.material,
        &liar.ids,
        PalwClaimRootsV1 { output_root: None, ..liar.roots() },
        located.binding.step_leaf_count,
        located.leaf,
        liar.backend.prompt_ids_form(),
    )
    .expect("the producer's leaf evidence");
    assert!(!evidence.artifact_openings.is_empty(), "a matmul reads model rows");
    let answer =
        |evidence: kaspa_consensus_core::palw_shard_court_v1::PalwLeafEvidenceV1| PalwConsensusObjectV2::MaterialDisclosedV2 {
            claim,
            unit: PalwDaUnitV1::Held(missing),
            answer: PalwDaAnswerV1::Held(Box::new(PalwHeldDisclosureCarriageV1 {
                version: PALW_HELD_DA_VERSION_V1,
                claim,
                missing,
                binding: located.binding.clone(),
                disclosure: PalwHeldDisclosureV1::StepLeaf { evidence: Box::new(evidence) },
                signature: Vec::new(),
            })),
            discloser: bond_key(PRODUCER),
            signature: SIG.to_vec(),
        };
    assert!(
        matches!(rc_step(&w.s, w.daa, &[answer(evidence.clone())]), Err(PalwStateV2Error::HeldDaRefused { .. })),
        "model rows are never compelled past the fence"
    );
    let stripped = kaspa_consensus_core::palw_shard_court_v1::PalwLeafEvidenceV1 { artifact_openings: Vec::new(), ..evidence };
    w.block(vec![answer(stripped)]).expect("the claim part answers by membership");
    assert!(matches!(phase_of(&w.s, &claim), PalwClaimPhaseV2::ReceiptLicensed { .. }), "an answer convicts nobody by itself");
    file_the_recompute(&mut w, &f, &liar, &o, &located, claim);
    let PalwClaimPhaseV2::Voided { reason, .. } = phase_of(&w.s, &claim) else { panic!("voided: {:?}", phase_of(&w.s, &claim)) };
    assert_eq!(reason, PalwVoidReasonV2::CourtFraud);
    assert!(bonds_collateral(&w.s).0 < before.0, "the producer is charged");
}

/// **Withholding is a default, never a fraud verdict** — a descent node the producer leaves unanswered, and on a second claim the
/// fused leaf's committed witness: each session defaults at its deadline (`ProducerWithholding`, DA-7).
#[tokio::test(flavor = "multi_thread")]
async fn lg14b_withheld_descent_and_witness_units_default_never_convict() {
    let f = Fixture::new(true);
    let liar = &f.d;
    for withhold_witness in [false, true] {
        let (s, claim) = rc_licensed(liar, &f.canonical, &f.profile, f.root);
        let mut w = World { s, daa: 104, chain: Vec::new() };
        let o = Outsider::start(&f, liar);
        let unit = if withhold_witness {
            let located = localize(&mut w, liar, &o, claim, false).expect("answered");
            let ckw = PalwLegacyHeldUnitV2::KernelWitness { leaf: located.leaf };
            w.block(vec![o.demand(claim, ckw, &located.binding)]).expect("demanded");
            ckw
        } else {
            localize(&mut w, liar, &o, claim, true).err().expect("the first descent node is withheld")
        };
        let session = w.s.da_sessions_of(&claim).find(|(a, _)| **a == bond_key(OUTSIDER)).map(|(_, s)| s.clone()).expect("open");
        assert!(session.units.contains(&PalwDaUnitV1::LegacyHeldV2(unit)));
        while w.daa <= session.deadline_daa + 1 && !matches!(phase_of(&w.s, &claim), PalwClaimPhaseV2::Voided { .. }) {
            w.quiet();
        }
        let PalwClaimPhaseV2::Voided { reason, .. } = phase_of(&w.s, &claim) else {
            panic!("defaulted: {:?}", phase_of(&w.s, &claim))
        };
        assert_eq!(reason, PalwVoidReasonV2::ProducerWithholding, "a default, not a fraud verdict ({unit:?})");
    }
}

/// **An honest claim and a malicious outsider**: an honest producer answers a step node and a checkpoint node from its own FOLD
/// (the retained level and a replayed block — the responder an honest node runs), every answer authenticates and the sessions are
/// refuted; the outsider's own descent finds nothing (its root is the claim's); a recompute of an honest leaf is a false accusation the
/// outsider pays; and the claim reaches `Final`.
#[tokio::test(flavor = "multi_thread")]
async fn lg14b_an_honest_claim_survives_a_malicious_outsider() {
    let f = Fixture::new(false);
    let honest = &f.d;
    let (s, claim) = rc_licensed(honest, &f.canonical, &f.profile, f.root);
    let mut w = World { s, daa: 104, chain: Vec::new() };
    let o = Outsider::start(&f, honest);
    assert_eq!(o.roots.execution_root, honest.execution_root, "an honest replica reproduces an honest claim");
    let binding = misaka_palw_base0::produce::base0_material_decode_any_v1(&honest.material).expect("decodes").binding().clone();
    let own = o.tree();
    assert_eq!(
        palw_legacy_descent_next_v2(Tree::Step, binding.step_leaf_count, &binding.step_merkle_root, &own, &[]),
        PalwLegacyDescentStepV2::Agrees
    );
    // The honest producer's own fold answers a malicious demand at the root and at a checkpoint node.
    let producer_tree = PalwLegacyTreeV2::fold_v1(&honest.backend, &honest.material, &honest.ids).expect("a fold");
    let checkpoint_tree = PalwLegacyTreeV2::leaves_v1(checkpoint_leaf_hashes(&honest.material));
    let height = kaspa_consensus_core::palw_tir_court_v1::palw_tir_step_tree_height_v1(binding.step_leaf_count);
    let c_height = kaspa_consensus_core::palw_tir_court_v1::palw_tir_step_tree_height_v1(u64::from(binding.checkpoint_count));
    let mut units = vec![PalwLegacyHeldUnitV2::StepNode { level: height, index: 0 }];
    if c_height >= 1 {
        units.push(PalwLegacyHeldUnitV2::CheckpointNode { level: c_height, index: 0 });
    }
    let before = bonds_collateral(&w.s);
    for unit in units {
        w.block(vec![o.demand(claim, unit, &binding)]).expect("a malicious demand still opens a session");
        let answer = palw_legacy_held_answer_v2(
            &unit,
            &producer_tree,
            &checkpoint_tree,
            &honest.backend,
            &honest.material,
            &honest.ids,
            honest.roots(),
            honest.backend.prompt_ids_form(),
        )
        .expect("an honest producer answers from its fold");
        let object = palw_legacy_held_answer_object_v2(&h64(999), claim, unit, binding.clone(), answer, bond_key(PRODUCER), |_, _| {
            Some(SIG.to_vec())
        })
        .expect("built");
        w.block(vec![object]).expect("an honest fold's frontier authenticates");
        assert!(w.s.da_sessions_of(&claim).next().is_none(), "refuted and closed");
    }
    let (replays, _) = producer_tree.resources();
    assert!(replays >= 1, "below the retained level the honest producer replays a block, never the whole run");
    // A recompute of an honest non-fused leaf: the court recomputes the committed tile — a false accusation.
    let leaf = matmul_leaf_at(&f.profile, &binding.job_context, 0, 3);
    let frontiers: Vec<PalwLegacyFrontierV2> = Vec::new();
    let view = PalwLegacyNodeViewV2 {
        leaf_count: binding.step_leaf_count,
        divergent: binding.step_leaf_count,
        frontiers: &frontiers,
        own: &own,
    };
    let accusation = palw_legacy_leaf_recompute_v2(
        &o.backend,
        &o.capture,
        &o.ids,
        o.roots,
        o.backend.prompt_ids_form(),
        &binding,
        &view,
        own.leaf_hash(leaf).expect("its own leaf"),
        leaf,
        claim,
        honest.trace_root,
        bond_key(PRODUCER),
        bond_key(OUTSIDER),
    )
    .expect("built");
    let bound_to =
        PalwOneMoveClaimV2 { execution_root: honest.execution_root, class_id: f.profile.shape_profile_id(), artifact_root: f.root };
    assert_eq!(
        palw_legacy_leaf_recompute_verdict_v2(&accusation, &bound_to, PALW_HELD_STEP_LADDER_V1),
        Ok(PalwShardCourtVerdictV1::FalseAccusation)
    );
    w.block(vec![palw_legacy_leaf_recompute_object_v2(&h64(999), accusation, |_, _| Some(SIG.to_vec())).expect("signed")])
        .expect("a false accusation folds and is charged");
    let after = bonds_collateral(&w.s);
    assert_eq!(after.0, before.0, "the honest producer is not charged");
    assert!(after.1 < before.1, "the malicious outsider pays");
    for _ in 0..2_000 {
        if matches!(phase_of(&w.s, &claim), PalwClaimPhaseV2::Final { .. }) {
            break;
        }
        w.quiet();
    }
    assert!(matches!(phase_of(&w.s, &claim), PalwClaimPhaseV2::Final { .. }), "the honest claim reaches Final");
}

/// **Below the fence the fold refuses every legacy held object by name and the state is untouched** — the unarmed twin of the
/// cases above: the same demand, answer and recompute, folded with the fence unset, are each `LegacyHeldDaDormant`, and a block
/// without them is byte-identical to the armed fold of the same block.
#[tokio::test(flavor = "multi_thread")]
async fn lg14b_below_the_fence_every_legacy_object_is_refused_and_the_state_is_untouched() {
    let f = Fixture::new(true);
    let liar = &f.d;
    let (s, claim) = rc_licensed(liar, &f.canonical, &f.profile, f.root);
    let binding = misaka_palw_base0::produce::base0_material_decode_any_v1(&liar.material).expect("decodes").binding().clone();
    let o = Outsider::start(&f, liar);
    let height = kaspa_consensus_core::palw_tir_court_v1::palw_tir_step_tree_height_v1(binding.step_leaf_count);
    let unit = PalwLegacyHeldUnitV2::StepNode { level: height, index: 0 };
    let objects = [o.demand(claim, unit, &binding), producer_answers(liar, &binding, claim, unit)];
    for object in &objects {
        assert!(kaspa_consensus_core::palw_state_v2::palw_object_is_legacy_held_da_v2(object));
        assert!(matches!(
            rc_step_with(&s, 104, std::slice::from_ref(object), None, false),
            Err(PalwStateV2Error::LegacyHeldDaDormant)
        ));
    }
    let unarmed = rc_step_with(&s, 104, &[], None, false).expect("an empty block");
    let armed = rc_step_with(&s, 104, &[], None, true).expect("an empty block");
    assert_eq!(unarmed.state_root(), armed.state_root(), "the flag alone moves nothing");
    // And the resource bound the design states for the canonical 8k row (≈105.5M leaves, the verifier's retained level 12).
    assert!(palw_legacy_verifier_resident_bytes_v2(105_500_000, 12) < 2_100_000);
    assert_eq!(palw_legacy_descent_rounds_v2(105_500_000), 3);
}
