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
    PalwLegacyTreeV2, palw_legacy_held_answer_object_v2, palw_legacy_held_answer_v2, palw_legacy_held_demand_object_v2,
    palw_legacy_leaf_recompute_object_v2, palw_legacy_leaf_recompute_v2,
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

/// R-core+ in force from genesis, as on testnet-12.
const RCORE_FROM_DAA: u64 = 0;
/// A third colluding seat: R-core+'s licence needs the colluding quorum (three backed `Valid`s).
const COLLUDER2: u64 = 7;

fn rc_params() -> PalwStateParamsV2 {
    // testnet-12's 500‰ work ceiling: A-6's accuser room is the free half above it (at the fixture's 1,000‰ nobody may accuse).
    params()
        .with_fp_exposure_ceiling(500)
        .expect("a ceiling")
        .with_rcore_plus_mirrors(Some(RCORE_FROM_DAA), 0, Vec::new())
        .with_legacy_public_filer_from_daa(Some(0))
}

fn rc_extras(armed: bool) -> PalwTransitionExtrasV1 {
    PalwTransitionExtrasV1 { legacy_held_da_v2_active: armed, objective_offence_daa: Some(0), ..launch() }
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
    // The consistency checker assumes R-core+ from genesis (testnet-12's case): a claim licensed at 103, before this harness's
    // crossing at 104, carries the pre-fence licence and no R-core+ licence door. That one finding is the crossing's, not a fold fault.
    if let Err(e) = child.assert_internal_consistency(&p) {
        assert!(format!("{e:?}").contains("records no licence door"), "internal consistency: {e:?}");
    }
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
    rc_licensed_at_header(d, canonical, profile, artifact_root, None)
}

fn rc_licensed_at_header(
    d: &Produced,
    canonical: &PalwJobContextV2,
    profile: &PalwShapeProfileV3,
    artifact_root: Hash64,
    job_anchor: Option<Hash64>,
) -> (PalwChainStateV2, Hash64) {
    rc_licensed_job(d, canonical, profile, artifact_root, job_anchor, None)
}

fn rc_licensed_job(
    d: &Produced,
    canonical: &PalwJobContextV2,
    profile: &PalwShapeProfileV3,
    artifact_root: Hash64,
    job_anchor: Option<Hash64>,
    fp_payload: Option<&kaspa_consensus_core::palw_freeprompt_v3::PalwFpCommitmentTxPayloadV3>,
) -> (PalwChainStateV2, Hash64) {
    let class_id = profile.shape_profile_id();
    let bond = |n: u64| PalwConsensusObjectV2::BondRegistered {
        bond: bond_key(n),
        pubkey: vec![n as u8; 4],
        operator_pubkey: op_key(20 + n),
        // Deep enough that each seat's R-core+ lock fits its work room (the backed subset licenses).
        collateral: 1_000_000_000_000_000,
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
        bond(COLLUDER2),
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
    let claim = fp_payload.map_or_else(|| attempt_id_v2(&env.attempt), |p| p.claim_id());
    let s = if let Some(payload) = fp_payload {
        let freeprompt = kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptParamsV3::new(
            kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_RECEIPT_V3,
            8,
            64,
            2048,
            128,
            600,
            600,
            1,
        )
        .unwrap();
        let tx = kaspa_consensus_core::tx::Transaction::new(
            kaspa_consensus_core::constants::TX_VERSION,
            vec![],
            vec![],
            0,
            kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_FP_COMMITMENT,
            0,
            borsh::to_vec(payload).unwrap(),
        );
        let extracted = kaspa_consensus_core::palw_fp_objects_v3::palw_fp_objects_from_accepted_txs_under_ruleset_v3(
            &[tx],
            h64(999),
            &freeprompt,
            point(101).block,
            false,
            LADDER,
            true,
            d.backend.prompt_ids_form(),
            |_, _, _, _| true,
        );
        assert!(extracted.skipped.is_empty(), "FP extraction: {:?}", extracted.skipped);
        assert_eq!(extracted.objects.len(), 1);
        rc_step(&s, 101, &[extracted.objects[0].object.clone()]).expect("the actual FP claim fold")
    } else if let Some(anchor) = job_anchor {
        // The older localizer fixtures have no header. This identity fixture threads the real v7 fold's header inputs,
        // rather than editing a claim's recorded identity after its acceptance.
        let p = rc_params();
        let extras = PalwTransitionExtrasV1 { own_job_anchor: anchor, ..rc_extras(true) };
        let key = kaspa_consensus_core::palw_attempt_v2::execution_commitment_v3(&env.attempt, anchor);
        let (child, delta, _) = kaspa_consensus_core::palw_state_v2::apply_palw_transition_v7(
            &s,
            &p,
            None,
            &point(101),
            &[],
            kaspa_consensus_core::palw_state_v2::PalwBlockWorkV3::Attempt(&env),
            &[],
            key,
            false,
            false,
            false,
            true,
            &extras,
        )
        .expect("the anchored claim");
        child.assert_internal_consistency(&p).expect("consistent anchored claim");
        child.assert_deadline_consistency(&p).expect("consistent deadlines");
        assert_eq!(apply_delta_v2(&s, &delta, &p).expect("reapplies"), child);
        assert_eq!(revert_delta_v2(&child, &delta, &p).expect("reverts"), s);
        assert_eq!(PalwStateCarriageV2::from_state(&child).into_state(&p, Some(child.state_root())).expect("reloads"), child);
        child
    } else {
        rc_step_with(&s, 101, &[], Some(&env), true).expect("the claim")
    };
    let seats = vec![
        PalwPanelSeatV2 { bond: bond_key(SEAT), operator_id: palw_operator_id_v2(&op_key(20 + SEAT)) },
        PalwPanelSeatV2 { bond: bond_key(COLLUDER), operator_id: palw_operator_id_v2(&op_key(20 + COLLUDER)) },
        PalwPanelSeatV2 { bond: bond_key(COLLUDER2), operator_id: palw_operator_id_v2(&op_key(20 + COLLUDER2)) },
    ];
    let s = rc_step(&s, 102, &[PalwConsensusObjectV2::PanelBound { claim, anchor: h64(77), seats }]).expect("the panel");
    let valid = |seat: u64| PalwSeatReceiptV2 {
        claim,
        verdict: PalwReceiptVerdictV2::Valid,
        seat_bond: bond_key(seat),
        signed_daa: 103,
        signature: Vec::new(),
    };
    let s = rc_step(
        &s,
        103,
        &[PalwConsensusObjectV2::ReceiptLicensed { claim, receipts: vec![valid(SEAT), valid(COLLUDER), valid(COLLUDER2)] }],
    )
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

/// The actual node filer's controller on this fold fixture. Its own replica comes from the outsider; historical answers come
/// only from public chain objects. The producer's obligations are built separately by `producer_answers`.
fn service_filer_case(
    f: &Fixture,
    d: &Produced,
    o: &Outsider,
    claim: Hash64,
) -> crate::palw_panel::palw_fraud_filer::PalwFraudFilerCaseV1 {
    use crate::palw_panel::palw_fraud_filer::{PalwFraudFilerCaseV1, PalwFraudFilerRunV1, PalwFraudFilerVerdictV1};
    let replica = Arc::new(crate::palw_legacy_held_v2::PalwLegacyReplicaV2 {
        backend: Arc::new(f.seat()),
        capture: Arc::new(o.capture.clone()),
        prompt_ids: Arc::new(o.ids.clone()),
        form: o.backend.prompt_ids_form(),
        roots: o.roots,
    });
    let mut case = PalwFraudFilerCaseV1::new(kaspa_consensus_core::palw_state_v2::PalwFraudFilerCandidateV1 {
        claim_id: claim,
        producer: bond_key(PRODUCER),
        accepted_daa: 101,
        seat: false,
        job: kaspa_consensus_core::palw_operator_da_v1::PalwOperatorDaJobV1 {
            accepted_block: point(101).block,
            class_id: f.profile.shape_profile_id(),
            artifact_root: Some(f.root),
            execution_root: d.execution_root,
            trace_root: d.trace_root,
            output_root: h64(32),
            work_leaves: d.committed_leaves.len() as u64,
            free_prompt: false,
            held_to_final: false,
        },
    });
    case.verdict = PalwFraudFilerVerdictV1::Mismatch(Arc::new(PalwFraudFilerRunV1 {
        execution_root: o.roots.execution_root,
        trace_root: o.roots.trace_root,
        output_root: o.roots.output_root.unwrap_or_default(),
        legacy: Some(replica),
        own_range: Box::new(|_, _| panic!("the LG14-B controller does not use the linear range fallback")),
        _reservation: None,
    }));
    case
}

fn service_public_answer(
    chain: &[PalwConsensusObjectV2],
    claim: Hash64,
    unit: PalwDaUnitV1,
    root: Hash64,
) -> Option<crate::palw_panel::PalwDaBuiltAnswerV1> {
    use crate::palw_panel::PalwDaBuiltAnswerV1;
    chain.iter().find_map(|object| match object {
        PalwConsensusObjectV2::MaterialDisclosedV2 { claim: held, unit: asked, answer, .. }
            if *held == claim
                && *asked == unit
                && kaspa_consensus_core::palw_legacy_public_filer_v1::palw_fraud_filer_answer_authenticates_v1(
                    &unit, answer, &root,
                ) =>
        {
            Some(PalwDaBuiltAnswerV1::Rcore(answer.clone()))
        }
        PalwConsensusObjectV2::LegacyHeldAnsweredV2 { answer }
            if answer.claim == claim
                && unit == PalwDaUnitV1::LegacyHeldV2(answer.unit)
                && kaspa_consensus_core::palw_legacy_held_da_v2::palw_legacy_held_check_answer_v2(
                    &root,
                    &answer.unit,
                    &answer.binding,
                    &answer.answer,
                    answer.binding.step_leaf_count,
                )
                .is_ok() =>
        {
            Some(PalwDaBuiltAnswerV1::LegacyHeldV2(Box::new((answer.binding.clone(), answer.answer.clone()))))
        }
        _ => None,
    })
}

fn service_filer_step(
    w: &World,
    case: &crate::palw_panel::palw_fraud_filer::PalwFraudFilerCaseV1,
) -> Result<crate::palw_panel::palw_fraud_filer::PalwFraudFilerStepV1, String> {
    use crate::palw_panel::palw_fraud_filer::palw_fraud_filer_legacy_step_v2;
    use kaspa_consensus_core::palw_legacy_public_filer_v1::{PalwFilerRoleV1, palw_fraud_filer_reservation_v1};
    let claim = case.candidate.claim_id;
    let view = w.s.palw_legacy_dispute_view_v1(&rc_params(), &claim).expect("public claim view");
    let reservable = kaspa_consensus_core::palw_state_v2::palw_legacy_dispute_reservation_check_v1(
        &w.s,
        &rc_params(),
        &rc_extras(true),
        &palw_fraud_filer_reservation_v1(&view, bond_key(OUTSIDER)),
        w.daa,
    )
    .is_ok();
    let court = palw_court_duties_v2(&w.s, &[bond_key(OUTSIDER)]).iter().any(|d| d.claim_id == claim && !d.i_am_responder);
    palw_fraud_filer_legacy_step_v2(PalwFilerRoleV1::PublicBond, Some(&view), &bond_key(OUTSIDER), reservable, court, case, |unit| {
        service_public_answer(&w.chain, claim, *unit, view.execution_root).is_some()
    })
}

fn service_filer_localize(w: &mut World, d: &Produced, case: &mut crate::palw_panel::palw_fraud_filer::PalwFraudFilerCaseV1) -> u64 {
    service_filer_drive(w, d, case, |_| false).expect("all demanded units are answered")
}

fn service_filer_drive(
    w: &mut World,
    d: &Produced,
    case: &mut crate::palw_panel::palw_fraud_filer::PalwFraudFilerCaseV1,
    withhold: impl Fn(PalwLegacyHeldUnitV2) -> bool,
) -> Result<u64, PalwLegacyHeldUnitV2> {
    use crate::palw_panel::palw_fraud_filer::PalwFraudFilerStepV1;
    use kaspa_consensus_core::palw_legacy_public_filer_v1::{
        PalwFilerActionV1, PalwLegacyProbeV1, palw_dispute_reserved_object_v1, palw_fraud_filer_demand_object_v1,
        palw_fraud_filer_reservation_v1,
    };
    for _ in 0..32 {
        let claim = case.candidate.claim_id;
        match service_filer_step(w, case).expect("the production controller can judge the own replica") {
            PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Reserve) => {
                let view = w.s.palw_legacy_dispute_view_v1(&rc_params(), &claim).unwrap();
                w.block(vec![palw_dispute_reserved_object_v1(
                    h64(999),
                    palw_fraud_filer_reservation_v1(&view, bond_key(OUTSIDER)),
                    |_| SIG.to_vec(),
                )])
                .expect("the reservation lands");
            }
            PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Demand(probe)) => {
                let demand = palw_fraud_filer_demand_object_v1(
                    &h64(999),
                    claim,
                    &d.execution_root,
                    probe,
                    case.binding.as_ref(),
                    bond_key(OUTSIDER),
                    d.backend.prompt_ids_form(),
                    |_, _| Some(SIG.to_vec()),
                )
                .expect("the node's builder");
                w.block(vec![demand]).expect("the unit opens a reserved session");
                assert!(
                    matches!(service_filer_step(w, case).unwrap(), PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Wait)),
                    "one open session at a time"
                );
                if let PalwLegacyProbeV1::HeldNode { unit } = probe {
                    if withhold(unit) {
                        return Err(unit);
                    }
                }
                let answer = match probe {
                    PalwLegacyProbeV1::Binding { row, tile } => PalwConsensusObjectV2::MaterialDisclosedV2 {
                        claim,
                        unit: probe.unit(),
                        answer: PalwDaAnswerV1::Event(
                            d.backend.disclose_trace_event(&d.material, row, tile).expect("producer's obligation"),
                        ),
                        discloser: bond_key(PRODUCER),
                        signature: SIG.to_vec(),
                    },
                    PalwLegacyProbeV1::HeldNode { unit } => producer_answers(d, case.binding.as_ref().unwrap(), claim, unit),
                    _ => panic!("the production LG14-B path never scans a linear range"),
                };
                w.block(vec![answer]).expect("the public answer authenticates");
            }
            PalwFraudFilerStepV1::Learn(probe) => {
                let answer = service_public_answer(&w.chain, claim, probe.unit(), d.execution_root).expect("public bytes");
                case.learn(probe, &answer).expect("the actual node learns only authenticated material");
            }
            PalwFraudFilerStepV1::LegacyTerminal { leaf } => return Ok(leaf),
            other => panic!("the public production controller stopped at {other:?}"),
        }
    }
    panic!("the production controller did not reach its terminal");
}

fn service_filer_terminal(
    f: &Fixture,
    d: &Produced,
    case: &crate::palw_panel::palw_fraud_filer::PalwFraudFilerCaseV1,
    leaf: u64,
) -> PalwConsensusObjectV2 {
    use crate::palw_panel::palw_fraud_filer::{PalwFraudFilerVerdictV1, palw_fraud_filer_sign_terminal_v2};
    let PalwFraudFilerVerdictV1::Mismatch(run) = &case.verdict else { panic!("own mismatched replica") };
    let bound_to =
        PalwOneMoveClaimV2 { execution_root: d.execution_root, class_id: f.profile.shape_profile_id(), artifact_root: f.root };
    let unsigned = run
        .legacy
        .as_ref()
        .unwrap()
        .terminal(
            case.binding.as_ref().unwrap(),
            &case.frontiers,
            case.witness.as_deref(),
            leaf,
            case.candidate.claim_id,
            &bound_to,
            d.trace_root,
            bond_key(PRODUCER),
            bond_key(OUTSIDER),
            PALW_HELD_STEP_LADDER_V1,
        )
        .expect("the node's court predicate finds an actionable terminal");
    assert!(
        palw_fraud_filer_sign_terminal_v2(unsigned.clone(), &h64(999), 0, |_, _| {
            panic!("the close ceiling is checked before signing")
        })
        .is_err()
    );
    assert!(palw_fraud_filer_sign_terminal_v2(unsigned.clone(), &h64(999), u64::MAX, |_, _| None).is_err());
    palw_fraud_filer_sign_terminal_v2(unsigned, &h64(999), u64::MAX, |_, _| Some(SIG.to_vec()))
        .expect("the node's terminal signs and rides")
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
        let (duty, needed) = held.history_filter_v1(&held_duties, read.session_id).unwrap();
        let cap = host.opening_cap(&duty.class_id, daa);
        let mut objects = std::collections::BTreeMap::new();
        for object in chain.iter().rev() {
            if let Some(key) = crate::palw_panel::held_court::palw_held_history_object_key_v1(object, &duty, &needed, cap) {
                objects.insert(key, object.clone());
            }
        }
        held.note_history_page_v1(
            read.session_id,
            read.claim_id,
            crate::palw_panel::held_court::PalwHeldHistoryPageV1 {
                walk: crate::palw_panel::held_court::PalwHeldHistoryWalkV1 {
                    floor: read.not_before_daa,
                    anchor: h64(daa),
                    next: None,
                    selection: crate::palw_panel::held_court::palw_held_history_selection_v1(&duty, &needed, cap),
                },
                reset: false,
                rewind: true,
                objects,
            },
            daa,
        );
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
    // Bounded by the replica's own retention, not the job: at most the block cache (four blocks of 2^retain) and the retained
    // levels — on this 11k-leaf fixture that is about the whole job, at the canonical 8k row it is 1.9 MB of 6.75 GB (unit test).
    let block = 1u64 << retain;
    assert!(located.resident_leaves <= 4 * block + 2 * n.div_ceil(block), "resident {} leaves", located.resident_leaves);
    assert!(located.replays <= u64::from(located.rounds) + 2, "{} blocks replayed in {} rounds", located.replays, located.rounds);
    // 2. Gap (c): the committed fused tile, disclosed by the unit DA-3 used to refuse.
    let ckw = PalwLegacyHeldUnitV2::KernelWitness { leaf: located.leaf };
    w.block(vec![o.demand(claim, ckw, &located.binding)]).expect("a fused leaf's witness is demandable");
    w.block(vec![producer_answers(liar, &located.binding, claim, ckw)]).expect("the witness authenticates by membership alone");
    let witness = public_witness(&w.chain, &claim, located.leaf).expect("public");
    assert!(witness.refutation.inputs.is_empty() && witness.refutation.kv_checkpoint.is_none(), "no history: the dissection's");
    assert!(w.s.da_sessions_of(&claim).next().is_none(), "every session answered and closed");
    // 3. The outsider's own kernel over its own history says the tile lies; it opens the held dissection.
    let mut service_case = service_filer_case(&f, liar, &o, claim);
    let service_leaf = service_filer_localize(&mut w, liar, &mut service_case);
    assert_eq!(service_leaf, located.leaf, "the production controller reads the already answered public descent and CKW");
    let opening = service_filer_terminal(&f, liar, &service_case, service_leaf);
    assert!(matches!(&opening, PalwConsensusObjectV2::ShardCourtAccused { .. }));
    w.block(vec![opening]).expect("the fold opens the held dissection on the outsider's accusation");
    assert_eq!(
        service_filer_step(&w, &service_case).unwrap(),
        crate::palw_panel::palw_fraud_filer::PalwFraudFilerStepV1::Engine(
            kaspa_consensus_core::palw_legacy_public_filer_v1::PalwFilerActionV1::Wait
        ),
        "the public controller leaves an open own court to the held loop"
    );
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
    assert_eq!(
        service_filer_step(&w, &service_case).unwrap(),
        crate::palw_panel::palw_fraud_filer::PalwFraudFilerStepV1::Engine(
            kaspa_consensus_core::palw_legacy_public_filer_v1::PalwFilerActionV1::Done(
                kaspa_consensus_core::palw_legacy_public_filer_v1::PalwFilerPhaseV1::Convicted
            )
        )
    );
}

/// An actual FP commitment/fold, all three colluding Valid receipts, and the production own-replay helper on a fresh model.
/// The verifier receives only the public payload and DA answers; it never opens the producer's capture.
#[tokio::test(flavor = "multi_thread")]
async fn lg14b_public_fp_bootstrap_own_replay_localizes_and_convicts_after_colluding_valid() {
    use crate::palw_panel::palw_fraud_filer::{
        PalwFraudFilerCaseV1, PalwFraudFilerVerdictV1, palw_fraud_filer_execute_v1, palw_fraud_filer_fp_input_v1,
    };
    use kaspa_consensus_core::palw_fp_execution_v3::palw_fp_commitment_from_context_v3;
    use kaspa_consensus_core::palw_freeprompt_v3::PalwFpCommitmentTxPayloadV3;
    let f = Fixture::new(false);
    let pick = |_: &PalwJobContextV2| 0;
    let liar = produce_at(&f.artifact, &f.profile, Some(&pick));
    let prompt: Vec<usize> = liar.ids.iter().map(|id| *id as usize).collect();
    let producer = liar
        .backend
        .execute_free_prompt_with_drill_fault_v2(
            &liar.fp_job,
            &prompt,
            kaspa_consensus_core::palw_backend::PalwFreePromptDrillFaultV1::Leaf { leaf: liar.leaf },
        )
        .unwrap();
    let payload = PalwFpCommitmentTxPayloadV3 {
        version: liar.fp_job.version,
        commitment: palw_fp_commitment_from_context_v3(&liar.fp_job, &liar.ctx, &producer, 999_999).unwrap(),
        prompt_token_ids: liar.ids.clone(),
        signature: vec![0x6B; kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
    };
    assert_eq!(payload.commitment.execution_root, liar.execution_root);
    for (after_final, withhold) in [(false, false), (true, false), (true, true)] {
        let (s, claim) = rc_licensed_job(&liar, &f.canonical, &f.profile, f.root, None, Some(&payload));
        let mut w = World { s, daa: 104, chain: vec![] };
        if after_final {
            for _ in 0..2_000 {
                if matches!(phase_of(&w.s, &claim), PalwClaimPhaseV2::Final { .. }) {
                    break;
                }
                w.quiet();
            }
            assert!(matches!(phase_of(&w.s, &claim), PalwClaimPhaseV2::Final { .. }));
        }
        if after_final {
            let p = rc_params();
            assert!(w.s.vesting_row(&claim).is_none(), "FP has no reward vesting row");
            assert!(w.s.palw_fraud_filer_candidates_v1(&p, &bond_key(OUTSIDER), w.daa).iter().any(|c| c.claim_id == claim));
            let expiry = w.s.panel_liability(&claim).unwrap().expiry_daa;
            assert!(!w.s.palw_fraud_filer_candidates_v1(&p, &bond_key(OUTSIDER), expiry).iter().any(|c| c.claim_id == claim));
            assert!(
                kaspa_consensus_core::palw_state_v2::palw_da_accusation_admissible_v2(
                    &w.s,
                    &p,
                    &rc_extras(true),
                    &claim,
                    &bond_key(OUTSIDER),
                    expiry
                )
                .is_err()
            );
            let view = w.s.palw_legacy_dispute_view_v1(&p, &claim).unwrap();
            let reservation =
                kaspa_consensus_core::palw_legacy_public_filer_v1::palw_fraud_filer_reservation_v1(&view, bond_key(OUTSIDER));
            assert!(
                kaspa_consensus_core::palw_state_v2::palw_legacy_dispute_reservation_check_v1(
                    &w.s,
                    &p,
                    &rc_extras(true),
                    &reservation,
                    expiry,
                )
                .is_err()
            );
            let dormant = p.clone().with_legacy_public_filer_from_daa(None);
            assert!(
                kaspa_consensus_core::palw_state_v2::palw_legacy_dispute_reservation_check_v1(
                    &w.s,
                    &dormant,
                    &rc_extras(true),
                    &reservation,
                    w.daa,
                )
                .is_err()
            );
            assert!(w.s.palw_fraud_filer_candidates_v1(&dormant, &bond_key(OUTSIDER), w.daa).is_empty());
            assert!(
                kaspa_consensus_core::palw_state_v2::palw_da_accusation_admissible_v2(
                    &w.s,
                    &dormant,
                    &rc_extras(true),
                    &claim,
                    &bond_key(OUTSIDER),
                    w.daa
                )
                .is_err()
            );
        }
        if after_final && !withhold {
            // Begin just inside the original liability window; every subsequent DA answer crosses that old expiry.
            let expiry = w.s.panel_liability(&claim).unwrap().expiry_daa;
            for _ in 0..2_000 {
                if w.daa >= expiry - 1 {
                    break;
                }
                w.quiet();
            }
            assert_eq!(w.daa, expiry - 1);
        }
        let target = kaspa_consensus_core::palw_offence_attribution_v1::palw_offence_target_v1(&w.s, &claim).unwrap();
        assert_eq!(target.job_identity, kaspa_consensus_core::palw_fp_execution_v3::palw_fp_job_pin_v1(&payload.commitment));
        assert_eq!(target.lane, Some(kaspa_consensus_core::palw_offence_attribution_v1::PalwClaimSourceKindV1::FreePrompt));
        let backend = f.seat();
        let form = backend.prompt_ids_form();
        let input = palw_fraud_filer_fp_input_v1(&backend, &payload, form, &[]).unwrap();
        let ceiling = backend.fp_job_context_v1(&input.job).unwrap();
        let run = tokio::task::spawn_blocking(move || {
            palw_fraud_filer_execute_v1(Box::new(backend), ceiling, input.prompt_ids, form, None, Some(input.job), None)
        })
        .await
        .unwrap()
        .unwrap();
        assert_ne!(run.execution_root, target.execution_root);
        assert!(run.legacy.is_some());
        let mut case = PalwFraudFilerCaseV1::new(kaspa_consensus_core::palw_state_v2::PalwFraudFilerCandidateV1 {
            claim_id: claim,
            producer: target.executor_bond,
            accepted_daa: 101,
            seat: false,
            job: kaspa_consensus_core::palw_operator_da_v1::PalwOperatorDaJobV1 {
                accepted_block: point(101).block,
                class_id: target.class_id,
                artifact_root: Some(target.artifact_root),
                execution_root: target.execution_root,
                trace_root: target.trace_root,
                output_root: target.output_root,
                work_leaves: payload.commitment.work_leaves,
                free_prompt: true,
                held_to_final: false,
            },
        });
        case.verdict = PalwFraudFilerVerdictV1::Mismatch(Arc::new(run));
        let before = bonds_collateral(&w.s);
        if withhold {
            service_filer_drive(&mut w, &liar, &mut case, |_| true).expect_err("the required public frontier is withheld");
            for _ in 0..2_000 {
                if matches!(phase_of(&w.s, &claim), PalwClaimPhaseV2::Voided { .. }) {
                    break;
                }
                w.quiet();
            }
            assert!(matches!(phase_of(&w.s, &claim), PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::ProducerWithholding, .. }));
            assert!(bonds_collateral(&w.s).0 < before.0);
            assert_eq!(bonds_collateral(&w.s).1, before.1);
            continue;
        }
        let leaf = service_filer_localize(&mut w, &liar, &mut case);
        if after_final {
            let view = w.s.palw_legacy_dispute_view_v1(&rc_params(), &claim).unwrap();
            assert!(
                w.s.panel_liability(&claim).unwrap().expiry_daa >= view.hard_deadline_daa,
                "the active pursuit holds FP liability"
            );
        }
        assert_eq!(leaf, liar.leaf);
        let terminal = service_filer_terminal(&f, &liar, &case, leaf);
        w.block(vec![terminal]).expect("the production FP terminal convicts");
        assert!(matches!(phase_of(&w.s, &claim), PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtFraud, .. }));
        assert!(bonds_collateral(&w.s).0 < before.0);
        assert_eq!(bonds_collateral(&w.s).1, before.1);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn lg14b_the_node_filer_convicts_model_copy_and_matmul_faults_before_and_after_final() {
    use crate::palw_panel::palw_fraud_filer::PalwFraudFilerStepV1;
    use kaspa_consensus_core::palw_legacy_public_filer_v1::{PalwFilerActionV1, PalwFilerPhaseV1};
    let f = Fixture::new(false);
    for (gather, after_final) in [(true, false), (true, true), (false, false), (false, true)] {
        let pick = |ctx: &PalwJobContextV2| if gather { 0 } else { matmul_leaf_at(&f.profile, ctx, 0, 3) };
        let liar = produce_at(&f.artifact, &f.profile, Some(&pick));
        let (s, claim) = rc_licensed(&liar, &f.canonical, &f.profile, f.root);
        let mut w = World { s, daa: 104, chain: Vec::new() };
        if after_final {
            while !matches!(phase_of(&w.s, &claim), PalwClaimPhaseV2::Final { .. }) {
                w.quiet();
            }
        }
        let before = bonds_collateral(&w.s);
        let o = Outsider::start(&f, &liar);
        let mut case = service_filer_case(&f, &liar, &o, claim);
        let leaf = service_filer_localize(&mut w, &liar, &mut case);
        assert_eq!(leaf, liar.leaf);
        assert!(case.witness.is_none(), "non-fused terminals use the own model and public leaf hash, no CKW");
        let object = service_filer_terminal(&f, &liar, &case, leaf);
        assert!(matches!(&object, PalwConsensusObjectV2::LegacyLeafRecomputedV2 { .. }));
        w.block(vec![object]).expect("the actual node's recompute convicts");
        assert!(matches!(phase_of(&w.s, &claim), PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtFraud, .. }));
        assert_eq!(
            service_filer_step(&w, &case).unwrap(),
            PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Done(PalwFilerPhaseV1::Convicted))
        );
        assert!(bonds_collateral(&w.s).0 < before.0);
        assert_eq!(bonds_collateral(&w.s).1, before.1);
    }
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

/// A public event from a borrowed execution proves a wrong job, including after a colluding Panel made the claim Final.
/// The fixture intentionally places a free-prompt execution on an attempt claim: the recorded header anchor is another job.
#[tokio::test]
async fn lg14b_a_public_binding_convicts_the_wrong_job_without_an_answered_session_before_and_after_final() {
    use crate::palw_panel::palw_fraud_filer::palw_fraud_filer_public_filing_v1;
    use kaspa_consensus_core::palw_offence_attribution_v1::{
        PalwExecutorRefutedEvidenceV1, PalwIdentityRulesV1, palw_offence_target_v1,
    };
    use kaspa_consensus_core::palw_offence_v1::PalwPanelContradictionV1;
    let f = Fixture::new(false);
    let d = produce(&f.artifact, &f.profile, false);
    // Producer publishes the authenticated event; the outsider only receives its public bytes, never this capture.
    let event = d.backend.disclose_trace_event(&d.material, 0, 0).expect("public event");
    for after_final in [false, true] {
        let (s, claim) = rc_licensed_at_header(&d, &f.canonical, &f.profile, f.root, Some(h64(0xA001)));
        let mut w = World { s, daa: 104, chain: Vec::new() };
        if after_final {
            for _ in 0..2_000 {
                if matches!(phase_of(&w.s, &claim), PalwClaimPhaseV2::Final { .. }) {
                    break;
                }
                w.quiet();
            }
            assert!(matches!(phase_of(&w.s, &claim), PalwClaimPhaseV2::Final { .. }));
        }
        assert!(w.s.palw_legacy_dispute_view_v1(&rc_params(), &claim).expect("view").answered.is_empty());
        let target = palw_offence_target_v1(&w.s, &claim).expect("recorded public target");
        let rules =
            PalwIdentityRulesV1 { prompt_ids_form: d.backend.prompt_ids_form(), base_class_id: h64(1), da_signer_liability: true };
        let answer = crate::palw_panel::PalwDaBuiltAnswerV1::Rcore(PalwDaAnswerV1::Event(event.clone()));
        let filing =
            palw_fraud_filer_public_filing_v1(&target, rules, &answer, Some(w.daa)).expect("proof builds").expect("wrong job");
        let PalwConsensusObjectV2::ObjectiveOffence { evidence, .. } = &filing.object else { panic!("kind 4") };
        let decoded: PalwExecutorRefutedEvidenceV1 = borsh::from_slice(evidence).expect("court evidence");
        assert!(matches!(decoded.contradiction, PalwPanelContradictionV1::IdentityMismatch { .. }));
        let before = bonds_collateral(&w.s);
        // The node hands this object to its reporter door; this fold assertion checks the conviction itself without reporter reward.
        w.block(vec![filing.object]).expect("the actual objective adjudicator convicts");
        let PalwClaimPhaseV2::Voided { reason, .. } = phase_of(&w.s, &claim) else { panic!("convicted") };
        assert_eq!(reason, PalwVoidReasonV2::CourtFraud);
        assert!(bonds_collateral(&w.s).0 < before.0);
        assert_eq!(bonds_collateral(&w.s).1, before.1);
    }
}

/// Output evidence is built only from the publicly authenticated event. An honest output and an unbound event never file.
#[tokio::test]
async fn lg14b_public_tiled_tokens_prove_output_mismatch_and_refuse_an_honest_or_unbound_twin() {
    use crate::palw_panel::palw_fraud_filer::palw_fraud_filer_public_filing_v1;
    use kaspa_consensus_core::palw_offence_attribution_v1::{
        PalwClaimSourceKindV1, PalwExecutorRefutedEvidenceV1, PalwIdentityRulesV1, PalwOffenceTargetV1,
    };
    use kaspa_consensus_core::palw_offence_v1::PalwPanelContradictionV1;
    let f = Fixture::new(false);
    let d = produce(&f.artifact, &f.profile, false);
    let event = d.backend.disclose_trace_event(&d.material, 0, 0).expect("public event");
    assert!(matches!(event, kaspa_consensus_core::palw_step_refute::PalwTraceEventDisclosureV1::Tiled { .. }));
    let binding = event.binding();
    let ids = match &event {
        kaspa_consensus_core::palw_step_refute::PalwTraceEventDisclosureV1::Tiled { generated_token_ids, .. } => generated_token_ids,
        _ => unreachable!(),
    };
    // Unit-level target for the correctly recorded FP job; the wrong-attempt-job fold test above uses actual state facts.
    let mut target = PalwOffenceTargetV1 {
        claim_id: h64(500),
        class_id: f.profile.shape_profile_id(),
        artifact_root: f.root,
        executor_bond: bond_key(PRODUCER),
        execution_root: binding.committed_execution_root,
        lane: Some(PalwClaimSourceKindV1::FreePrompt),
        segment_count: None,
        phase: None,
        job_identity: kaspa_consensus_core::palw_fp_execution_v3::palw_fp_job_pin_of_context_v1(&binding.job_context),
        trace_root: binding.full_logits_trace_root,
        output_root: kaspa_consensus_core::palw_attempt_rules_v1::palw_attempt_output_root_v1(&binding.job_context, ids),
    };
    let rules = PalwIdentityRulesV1 { prompt_ids_form: d.backend.prompt_ids_form(), base_class_id: h64(1), da_signer_liability: true };
    let answer = crate::palw_panel::PalwDaBuiltAnswerV1::Rcore(PalwDaAnswerV1::Event(event.clone()));
    assert!(palw_fraud_filer_public_filing_v1(&target, rules, &answer, None).expect("honest").is_none());
    target.output_root = h64(501);
    let filing = palw_fraud_filer_public_filing_v1(&target, rules, &answer, None).expect("bound").expect("wrong output");
    let PalwConsensusObjectV2::ObjectiveOffence { evidence, .. } = filing.object else { panic!("kind 4") };
    let decoded: PalwExecutorRefutedEvidenceV1 = borsh::from_slice(&evidence).expect("court evidence");
    assert!(matches!(decoded.contradiction, PalwPanelContradictionV1::OutputMismatch { .. }));
    // A newer carrier may have a real binding but garbage token bytes. Across two pages the good historical pin wins.
    let mut junk = event.clone();
    if let kaspa_consensus_core::palw_step_refute::PalwTraceEventDisclosureV1::Tiled { generated_token_ids, .. } = &mut junk {
        generated_token_ids[0] = generated_token_ids[0].wrapping_add(1);
    }
    let junk = crate::palw_panel::PalwDaBuiltAnswerV1::Rcore(PalwDaAnswerV1::Event(junk));
    assert!(palw_fraud_filer_public_filing_v1(&target, rules, &junk, None).is_err());
    let key = (target.claim_id, PalwDaUnitV1::Event { row: 0, tile: 0 });
    let mut cache = BTreeMap::new();
    crate::palw_panel::palw_fraud_filer::palw_fraud_filer_cache_answer_v1(&mut cache, key, junk.clone());
    crate::palw_panel::palw_fraud_filer::palw_fraud_filer_cache_answer_v1(&mut cache, key, answer.clone());
    crate::palw_panel::palw_fraud_filer::palw_fraud_filer_cache_answer_v1(&mut cache, key, junk);
    assert_eq!(cache.len(), 1);
    assert!(palw_fraud_filer_public_filing_v1(&target, rules, &cache[&key], None).expect("good historical pin retained").is_some());
    // Count-only faults are terminal from the authenticated binding, before descent compares tree widths.
    let mut count_event = event.clone();
    let changed = match &mut count_event {
        kaspa_consensus_core::palw_step_refute::PalwTraceEventDisclosureV1::Tiled { binding, .. } => binding,
        _ => unreachable!(),
    };
    changed.step_leaf_count += 1;
    changed.committed_execution_root = kaspa_consensus_core::palw_step_leg::binding_commitment_root_v1(changed);
    target.execution_root = changed.committed_execution_root;
    let count_answer = crate::palw_panel::PalwDaBuiltAnswerV1::Rcore(PalwDaAnswerV1::Event(count_event));
    let proof =
        palw_fraud_filer_public_filing_v1(&target, rules, &count_answer, None).expect("count proof").expect("noncanonical count");
    let PalwConsensusObjectV2::ObjectiveOffence { evidence, .. } = proof.object else { panic!("kind 4") };
    let decoded: PalwExecutorRefutedEvidenceV1 = borsh::from_slice(&evidence).expect("count evidence");
    assert!(matches!(decoded.contradiction, PalwPanelContradictionV1::StepStructural(_)));
    target.execution_root = h64(502);
    assert!(palw_fraud_filer_public_filing_v1(&target, rules, &answer, None).is_err());
}

/// A shape-correct synthetic execution commitment, used only to exercise objective input attribution.
fn public_prompt_binding(
    profile: &PalwShapeProfileV3,
    anchor: Hash64,
    prompt_anchor: Hash64,
    form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
) -> PalwStepBindingV2 {
    use kaspa_consensus_core::palw_attempt_rules_v1::{
        palw_attempt_canonical_v1, palw_attempt_context_v1, palw_attempt_prompt_root_v1, palw_canonical_checkpoint_profile_v1,
        palw_int_activation_leg_root_v1,
    };
    let canonical = palw_attempt_canonical_v1(profile, false).expect("model canonical job");
    let prompt_root = palw_attempt_prompt_root_v1(&profile, &prompt_anchor, canonical.0, form).expect("public prompt root");
    let ctx = palw_attempt_context_v1(&profile, &anchor, canonical, prompt_root);
    let checkpoint_profile = palw_canonical_checkpoint_profile_v1(&profile);
    let checkpoint_count =
        kaspa_consensus_core::palw_context_ladder::palw_checkpoint_count_v1(&profile, &ctx, checkpoint_profile.checkpoint_interval);
    let mut binding = PalwStepBindingV2 {
        version: kaspa_consensus_core::palw_step_leg::PALW_STEP_LEG_OBJECT_VERSION_V1,
        state_chunk_map_id: profile.state_chunk_map_id,
        shape_profile: profile.clone(),
        checkpoint_profile,
        activation_leg_root: palw_int_activation_leg_root_v1(&ctx),
        step_leaf_count: kaspa_consensus_core::palw_step::step_leaf_count_capped_v1(&profile, &ctx, 1 << 40).expect("count"),
        step_merkle_root: h64(0x1302),
        checkpoint_count,
        checkpoint_merkle_root: if checkpoint_count == 0 {
            kaspa_consensus_core::palw_step_leg::checkpoint_empty_root_v2(&ctx.context_hash())
        } else {
            h64(0x1303)
        },
        full_logits_trace_root: h64(0x1304),
        job_context: ctx,
        committed_execution_root: Hash64::default(),
    };
    binding.committed_execution_root = kaspa_consensus_core::palw_step_leg::binding_commitment_root_v1(&binding);
    binding
}

/// The committed input is another anchor's prompt. Neither the prompt ids nor a producer capture are needed by this filer.
/// Synthetic bindings at the maximum context test the input-proof route, not execution of a maximum-size model.
#[tokio::test]
async fn lg14b_public_binding_proves_large_prompt_relabel_and_refuses_honest_or_unbound_inputs() {
    use crate::palw_panel::palw_fraud_filer::palw_fraud_filer_public_filing_v1;
    use kaspa_consensus_core::palw_attempt_rules_v1::palw_attempt_canonical_v1;
    use kaspa_consensus_core::palw_offence_attribution_v1::{
        PalwClaimSourceKindV1, PalwExecutorRefutedEvidenceV1, PalwIdentityRulesV1, PalwOffenceTargetV1,
    };
    use kaspa_consensus_core::palw_offence_v1::{PalwPanelContradictionV1, PalwPromptProofV1};
    use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
    use kaspa_consensus_core::palw_step_refute::PalwTraceEventDisclosureV1;
    let form = PalwPromptIdsFormV1::MerkleV1;
    let anchor = h64(0x1300);
    let rules = PalwIdentityRulesV1 { prompt_ids_form: form, base_class_id: h64(1), da_signer_liability: true };
    for n_ctx in [65_536, 2_097_152] {
        let mut profile = kaspa_consensus_core::palw_base0_profile::base0_profile_v1(
            kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_GEOMETRY,
        )
        .expect("floor graph");
        profile.n_ctx = n_ctx;
        let canonical = palw_attempt_canonical_v1(&profile, false).expect("canonical job");
        assert!(canonical.0 > 4096);
        for wrong_prompt in [false, true] {
            let prompt_anchor = if wrong_prompt { h64(0x1301) } else { anchor };
            let binding = public_prompt_binding(&profile, anchor, prompt_anchor, form);
            let mut target = PalwOffenceTargetV1 {
                claim_id: h64(0x1305),
                class_id: profile.shape_profile_id(),
                artifact_root: h64(0x1306),
                executor_bond: bond_key(PRODUCER),
                execution_root: binding.committed_execution_root,
                lane: Some(PalwClaimSourceKindV1::Attempt),
                segment_count: None,
                phase: None,
                job_identity: anchor,
                trace_root: binding.full_logits_trace_root,
                output_root: h64(0x1307),
            };
            // A carrier with only an authenticated binding is sufficient; the invalid absence pin is never trusted as DA.
            let answer =
                crate::palw_panel::PalwDaBuiltAnswerV1::Rcore(PalwDaAnswerV1::Event(PalwTraceEventDisclosureV1::OutOfRange {
                    binding: Box::new(binding),
                }));
            let filing = palw_fraud_filer_public_filing_v1(&target, rules, &answer, None).expect("bound input");
            if wrong_prompt {
                let filing = filing.expect("the canonical-context prompt relabel convicts");
                let PalwConsensusObjectV2::ObjectiveOffence { kind, evidence, .. } = &filing.object else { panic!("kind 4") };
                let decoded: PalwExecutorRefutedEvidenceV1 = borsh::from_slice(evidence).expect("input evidence");
                assert!(matches!(
                    decoded.contradiction,
                    PalwPanelContradictionV1::PromptNotAnchored { proof: PalwPromptProofV1::Whole, .. }
                ));
                assert_eq!(
                    kaspa_consensus_core::palw_offence_attribution_v1::palw_offence_heavy_prompt_ids_v1(*kind, evidence, false),
                    u64::from(canonical.0),
                    "the proof retains its heavy gate charge"
                );
            } else {
                assert!(filing.is_none(), "an honest input produces no proof, even with a malformed pin");
            }
            target.execution_root = h64(0xBAD);
            assert!(palw_fraud_filer_public_filing_v1(&target, rules, &answer, None).is_err());
        }
    }
}

/// The same input proof passes actual attribution after all Panel seats licensed the claim, before and after Final.
/// Class admission is the existing test fixture; this is a fold test, not maximum-profile execution or normal eligibility.
#[tokio::test]
async fn lg14b_large_prompt_relabel_convicts_after_colluding_valid_before_and_after_final() {
    use crate::palw_panel::palw_fraud_filer::palw_fraud_filer_public_filing_v1;
    use kaspa_consensus_core::palw_offence_attribution_v1::{PalwIdentityRulesV1, palw_offence_target_v1};
    use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
    use kaspa_consensus_core::palw_step_refute::PalwTraceEventDisclosureV1;
    for n_ctx in [65_536, 2_097_152] {
        let f = Fixture::new(false);
        let mut profile = f.profile.clone();
        profile.n_ctx = n_ctx;
        let form = PalwPromptIdsFormV1::MerkleV1;
        let anchor = h64(0x1310);
        let binding = public_prompt_binding(&profile, anchor, h64(0x1311), form);
        let d = Produced {
            ctx: binding.job_context.clone(),
            execution_root: binding.committed_execution_root,
            trace_root: binding.full_logits_trace_root,
            ..f.d
        };
        let canonical = public_prompt_binding(&profile, anchor, anchor, form).job_context;
        let answer = crate::palw_panel::PalwDaBuiltAnswerV1::Rcore(PalwDaAnswerV1::Event(PalwTraceEventDisclosureV1::OutOfRange {
            binding: Box::new(binding),
        }));
        for after_final in [false, true] {
            let (s, claim) = rc_licensed_at_header(&d, &canonical, &profile, f.root, Some(anchor));
            let mut w = World { s, daa: 104, chain: Vec::new() };
            if after_final {
                for _ in 0..2_000 {
                    if matches!(phase_of(&w.s, &claim), PalwClaimPhaseV2::Final { .. }) {
                        break;
                    }
                    w.quiet();
                }
                assert!(matches!(phase_of(&w.s, &claim), PalwClaimPhaseV2::Final { .. }));
            }
            let target = palw_offence_target_v1(&w.s, &claim).expect("recorded header target");
            let rules = PalwIdentityRulesV1 { prompt_ids_form: form, base_class_id: h64(1), da_signer_liability: true };
            let filing = palw_fraud_filer_public_filing_v1(&target, rules, &answer, None).expect("public proof").expect("wrong input");
            let before = bonds_collateral(&w.s);
            w.block(vec![filing.object]).expect("objective input conviction");
            assert!(matches!(phase_of(&w.s, &claim), PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtFraud, .. }));
            assert!(bonds_collateral(&w.s).0 < before.0);
            assert_eq!(bonds_collateral(&w.s).1, before.1);
        }
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
    use crate::palw_panel::palw_fraud_filer::PalwFraudFilerStepV1;
    use kaspa_consensus_core::palw_legacy_public_filer_v1::{PalwFilerActionV1, PalwFilerPhaseV1};
    let f = Fixture::new(true);
    let liar = &f.d;
    for withhold_witness in [false, true] {
        let (s, claim) = rc_licensed(liar, &f.canonical, &f.profile, f.root);
        let mut w = World { s, daa: 104, chain: Vec::new() };
        let o = Outsider::start(&f, liar);
        let mut case = service_filer_case(&f, liar, &o, claim);
        let unit = service_filer_drive(&mut w, liar, &mut case, |unit| {
            matches!(unit, PalwLegacyHeldUnitV2::KernelWitness { .. }) == withhold_witness
        })
        .expect_err("the selected obligation is withheld");
        let session = w.s.da_sessions_of(&claim).find(|(a, _)| **a == bond_key(OUTSIDER)).map(|(_, s)| s.clone()).expect("open");
        assert!(session.units.contains(&PalwDaUnitV1::LegacyHeldV2(unit)));
        while w.daa <= session.deadline_daa + 1 && !matches!(phase_of(&w.s, &claim), PalwClaimPhaseV2::Voided { .. }) {
            w.quiet();
        }
        let PalwClaimPhaseV2::Voided { reason, .. } = phase_of(&w.s, &claim) else {
            panic!("defaulted: {:?}", phase_of(&w.s, &claim))
        };
        assert_eq!(reason, PalwVoidReasonV2::ProducerWithholding, "a default, not a fraud verdict ({unit:?})");
        assert_eq!(
            service_filer_step(&w, &case).unwrap(),
            PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Done(PalwFilerPhaseV1::DaDefault))
        );
    }
}

/// **An honest claim and a malicious outsider**: an honest producer answers a step node and a checkpoint node from its own FOLD
/// (the retained level and a replayed block — the responder an honest node runs), every answer authenticates and the sessions are
/// refuted; the outsider's own descent finds nothing (its root is the claim's); a recompute of an honest leaf is a false accusation the
/// outsider pays; and the claim reaches `Final`.
#[tokio::test(flavor = "multi_thread")]
async fn lg14b_an_honest_claim_survives_a_malicious_outsider() {
    use crate::palw_panel::{PalwDaClaimFactsV1, PalwDaLaneV1, palw_da_built_answer_object_v1, palw_da_claim_answers_v1};
    let f = Fixture::new(false);
    let honest = &f.d;
    let (s, claim) = rc_licensed(honest, &f.canonical, &f.profile, f.root);
    let mut w = World { s, daa: 104, chain: Vec::new() };
    let o = Outsider::start(&f, honest);
    assert_eq!(o.roots.execution_root, honest.execution_root, "an honest replica reproduces an honest claim");
    let binding = misaka_palw_base0::produce::base0_material_decode_any_v1(&honest.material).expect("decodes").binding().clone();
    // Step-tree equality alone is not a complete judgement: a job, trace or checkpoint mismatch must take its own terminal.
    // Force the mismatch state to exercise the actual controller's guard, without claiming an honest job is fraudulent.
    let mut unresolved = service_filer_case(&f, honest, &o, claim);
    unresolved.binding = Some(binding.clone());
    let mut guard_world = World { s: w.s.clone(), daa: w.daa, chain: Vec::new() };
    let view = guard_world.s.palw_legacy_dispute_view_v1(&rc_params(), &claim).unwrap();
    guard_world
        .block(vec![kaspa_consensus_core::palw_legacy_public_filer_v1::palw_dispute_reserved_object_v1(
            h64(999),
            kaspa_consensus_core::palw_legacy_public_filer_v1::palw_fraud_filer_reservation_v1(&view, bond_key(OUTSIDER)),
            |_| SIG.to_vec(),
        )])
        .unwrap();
    assert!(service_filer_step(&guard_world, &unresolved).unwrap_err().contains("checkpoint/trace/job localization"));
    let own = o.tree();
    assert_eq!(
        palw_legacy_descent_next_v2(Tree::Step, binding.step_leaf_count, &binding.step_merkle_root, &own, &[]),
        PalwLegacyDescentStepV2::Agrees
    );
    // The honest producer's own fold answers a malicious demand at the root and at a checkpoint node.
    let producer_tree = PalwLegacyTreeV2::fold_v1(&honest.backend, &honest.material, &honest.ids).expect("a fold");
    let height = kaspa_consensus_core::palw_tir_court_v1::palw_tir_step_tree_height_v1(binding.step_leaf_count);
    let c_height = kaspa_consensus_core::palw_tir_court_v1::palw_tir_step_tree_height_v1(u64::from(binding.checkpoint_count));
    let mut units = vec![PalwLegacyHeldUnitV2::StepNode { level: height, index: 0 }];
    if c_height >= 1 {
        units.push(PalwLegacyHeldUnitV2::CheckpointNode { level: c_height, index: 0 });
    }
    units.push(PalwLegacyHeldUnitV2::KernelWitness { leaf: honest.leaf });
    let before = bonds_collateral(&w.s);
    let facts = PalwDaClaimFactsV1 {
        claim_id: claim,
        class_id: honest.fp_job.class_id,
        executor_bond: bond_key(PRODUCER),
        execution_root: honest.execution_root,
        trace_root: honest.trace_root,
        work_leaves: binding.step_leaf_count,
        form: honest.backend.prompt_ids_form(),
        lane: PalwDaLaneV1::FreePrompt { panel_da_admissible: false },
        job_pin: None,
    };
    for unit in units {
        w.block(vec![o.demand(claim, unit, &binding)]).expect("a malicious demand still opens a session");
        let mut built = palw_da_claim_answers_v1(
            &honest.backend,
            &facts,
            [honest.served()],
            |_| panic!("the kept honest capture already verifies"),
            &[PalwDaUnitV1::LegacyHeldV2(unit)],
            binding.job_context.exact_decode_tokens,
            false,
            None,
        )
        .expect("the node worker answers from verified fold retention");
        assert!(!built.remade);
        let answer = built.answers.pop().expect("one duty").expect("not covered by a Flat").expect("the fold opens");
        let object = palw_da_built_answer_object_v1(
            &h64(999),
            claim,
            PalwDaUnitV1::LegacyHeldV2(unit),
            answer,
            bond_key(PRODUCER),
            u64::MAX,
            |_, _| Some(SIG.to_vec()),
        )
        .expect("built");
        assert!(matches!(&object, PalwConsensusObjectV2::LegacyHeldAnsweredV2 { .. }), "the worker sends tag 158");
        w.block(vec![object]).expect("an honest fold's frontier authenticates");
        assert!(w.s.da_sessions_of(&claim).next().is_none(), "refuted and closed");
    }
    // The same fold reader used by the worker does block replay below its retained level.
    producer_tree.leaf_hash(0).expect("replayed from the honest retention");
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
