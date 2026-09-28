//! **RFC-0002 Phase F (F6, node half), item 5: an IR class through the node's doors and the fold.**
//!
//! A tiny IR class — the golden `dense-gqa-2layer` program (every cone within the terminal
//! ceiling, so no dissection), its params, a declared layout — is written as a `PALWTIR1` container
//! and loaded through the SDK's one door into the node's backend registry. Everything the node does
//! with it goes through the doors kaspad uses: `resolve` (the trait door: job, execute, verify,
//! replay, bisection) and `resolve_tir_v1` (the IR close). The chain is the real transition
//! (`apply_palw_transition_v2_with_extras`) and the real adjudicator (`adjudicate_court_close_v3`).
//!
//! * **Registration** — the node's `ClassRegisteredTirV1` (SDK builder, signed by the registrant
//!   bond) is REFUSED by today's fold by name: admission v10 and the IR class record are F6's
//!   consensus half. Until they land the class is stood in by a carriage-less `ClassRegistered`
//!   of the same `(class_id, artifact_root)`, so everything downstream of registration runs now.
//! * **An honest claim goes `Final`** — produced from the anchor's job, the served capture answers
//!   for the claim's roots, the seats' replay reproduces them, the panel licenses it; a false
//!   accusation against it walks the ladder to a leaf the responder's `TirCone` close acquits
//!   (`ChallengerDefeated`); the claim is final past the window.
//! * **A planted lie is convicted by the IR court** — one lane of one committed leaf moved and the
//!   commitment re-derived over it: the seat's replay does not reproduce the claim, it opens a
//!   court, the ladder (both parties' prefix states) lands exactly on the lie, the node's close
//!   (`palw_tir_close_candidates_v1`) is a `TirCone` the adjudicator reads `ExecutorGuilty`, and
//!   the fold voids the claim and charges the producer.

use super::tir_court::{palw_tir_close_candidates_v1, palw_tir_close_is_mine_v1};
use crate::palw_backends::PalwBackendRegistry;
use kaspa_consensus_core::palw_attempt_v2::{
    PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, attempt_id_v2, challenge_v2,
};
use kaspa_consensus_core::palw_backend::{PalwClaimRootsV1, PalwExecutionBackendV1, PalwMaterialVerdictV1};
use kaspa_consensus_core::palw_bisect::PalwBisectSpaceV1;
use kaspa_consensus_core::palw_bisect::{
    PALW_BISECT_OBJECT_VERSION_V1, PalwBisectDisclosureV1, PalwBisectTurnV1, PalwBisectVerdictV1,
};
use kaspa_consensus_core::palw_court_v2::{PalwCourtVerdictProofV2, court_session_id_v2};
use kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2;
use kaspa_consensus_core::palw_panel_v2::{PalwReceiptVerdictV2, PalwSeatReceiptV2};
use kaspa_consensus_core::palw_producer_v2::{PalwCourtDutyV2, palw_court_duties_v2};
use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwConsensusObjectV2, PalwCourtVerdictV2, PalwPanelSeatV2,
    PalwPwuRuleV2, PalwStateParamsV2, PalwStateV2Error, PalwTransitionExtrasV1, apply_palw_transition_v2_with_extras,
    palw_operator_id_v2,
};
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use kaspa_hashes::Hash64;
use misaka_palw_sdk::lineages::tir::TirBackendV1;
use std::path::PathBuf;

const NETWORK: &[u8] = b"misaka-palw-rc";
const PRODUCER: u64 = 1;
const SEAT: u64 = 2;
const COLLUDER: u64 = 4;
/// The court's ladder: the step-leaf space every session is opened over (the node opens at the
/// ruleset's `max_step_leaf_count`).
const LADDER: u64 = 1 << 26;
const TURN: u64 = 20;
const FORM: PalwPromptIdsFormV1 = PalwPromptIdsFormV1::Flat;

fn h64(v: u64) -> Hash64 {
    Hash64::from_u64_word(v)
}

fn bond_key(v: u64) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(v), index: 0 })
}

fn op_key(v: u64) -> Vec<u8> {
    vec![v as u8; 8]
}

fn court() -> PalwCourtParamsV2 {
    PalwCourtParamsV2::new(LADDER, TURN, 2).expect("a court")
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex")).collect()
}

/// The tiny IR class, as a node holds it.
struct Ir {
    dir: PathBuf,
    registry: PalwBackendRegistry,
    class_id: Hash64,
    root: Hash64,
}

impl Drop for Ir {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.dir).ok();
    }
}

impl Ir {
    fn backend(&self) -> Box<dyn PalwExecutionBackendV1> {
        self.registry.resolve(self.class_id, self.root).expect("the trait door serves the IR class")
    }

    fn tir(&self) -> TirBackendV1 {
        self.registry.resolve_tir_v1(self.class_id, self.root).expect("an IR class").expect("its backend")
    }
}

/// **The class**: `dense-gqa-2layer` from the golden program vectors under the `tiled` or flat
/// logits scheme, a layout of 8-lane commit tiles (the logits node at the tiled scheme's 4,096
/// lanes), two-position checkpoints and four-row history tiles, written as a `PALWTIR1` container
/// and loaded through the SDK's door (`PalwClassSdk::load_artifact`, dispatched by the magic).
fn ir_class(tag: &str, tiled: bool) -> Ir {
    use kaspa_consensus_core::palw_step_refute::{PALW_LOGITS_TILE_LANES, flat_logits_scheme_id_v1, tiled_logits_scheme_id_v1};
    use kaspa_consensus_core::palw_tir_class_v1::{PALW_TIR_LAYOUT_VERSION_V1, PalwTirLayoutV1};
    use misaka_palw_tir::TirProgramV1;
    let vector = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../consensus-vectors/tir-v1/programs/dense-gqa-2layer.json");
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&vector).expect("the golden program")).expect("json");
    let mut program = TirProgramV1::decode_canonical(&unhex(v["program_borsh_hex"].as_str().unwrap())).expect("canonical");
    let scheme = if tiled { tiled_logits_scheme_id_v1() } else { flat_logits_scheme_id_v1() };
    program.logits_scheme_id.copy_from_slice(scheme.as_byte_slice());
    let program = TirProgramV1::decode_canonical(&program.encode()).expect("still canonical");
    let mut tensors = std::collections::BTreeMap::new();
    for t in v["params"].as_array().unwrap() {
        tensors.insert(
            (t["param"].as_u64().unwrap() as u16, t["layer"].as_u64().map(|l| l as u16)),
            unhex(t["le_hex"].as_str().unwrap()),
        );
    }
    let mut commit_tiles = Vec::new();
    for (bi, b) in program.blocks.iter().enumerate() {
        for (ni, n) in b.nodes.iter().enumerate() {
            if n.commit {
                let logits = bi == program.schedule.post as usize && ni == program.logits as usize;
                commit_tiles.push(if logits && tiled { PALW_LOGITS_TILE_LANES as u32 } else { 8 });
            }
        }
    }
    let layout = PalwTirLayoutV1 {
        version: PALW_TIR_LAYOUT_VERSION_V1,
        max_context: 64,
        checkpoint_interval: 2,
        h_tile: 4,
        commit_tiles,
        state_tiles: program.states.iter().map(|_| 4).collect(),
    };
    let dir = std::env::temp_dir().join(format!("kaspad-tir-e2e-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("tiny.palwtir");
    let meta = serde_json::json!({ "model_id": format!("test/tiny-ir-{tag}") }).to_string();
    misaka_palw_tir_artifact::write_container_v1(&path, &program, borsh::to_vec(&layout).unwrap(), [9; 64], meta, &mut |j, l| {
        tensors.get(&(j, l)).cloned().ok_or_else(|| format!("no tensor {j} {l:?}"))
    })
    .expect("the container");
    let sdk = misaka_palw_sdk::PalwClassSdk::builtin_v1(court(), FORM, NETWORK.to_vec());
    let holding = sdk.load_artifact(&path).expect("the IR lineage loads it by its magic");
    let registry = PalwBackendRegistry::new(court(), FORM, vec![holding], NETWORK.to_vec());
    let entry = misaka_palw_sdk::tir_registration::tir_entries_of_v1(registry.holdings()).remove(0);
    Ir { dir, class_id: entry.class_id(), root: entry.artifact_root, registry }
}

// ---- the fold ------------------------------------------------------------------------------------

fn params() -> PalwStateParamsV2 {
    PalwStateParamsV2::new(100, 10, 10, 20, 600, 1000, h64(1), 4, 1000, 10_000, 1000, 0)
        .unwrap()
        .with_fp_quanta(8, 64)
        .unwrap()
        .with_turn_deadline_daa(TURN)
        .unwrap()
        .with_worker_carve_permille(300)
        .unwrap()
        // `palw_tir_v1` in force from genesis: the fold applies IR closes.
        .with_tir_from_daa(Some(0))
}

fn point(daa: u64) -> PalwBlockContextV2 {
    PalwBlockContextV2 { block: Hash64::from_u64_word(0x71C0_0000 + daa), daa_score: daa, blue_score: daa, subsidy: 10_000 }
}

fn step_with(
    parent: &PalwChainStateV2,
    daa: u64,
    objects: &[PalwConsensusObjectV2],
    att: Option<&PalwAttemptEnvelopeV2>,
) -> Result<PalwChainStateV2, PalwStateV2Error> {
    let p = params();
    let (child, _) = apply_palw_transition_v2_with_extras(
        parent,
        &p,
        &point(daa),
        objects,
        att,
        false,
        false,
        false,
        true,
        &PalwTransitionExtrasV1::default(),
    )?;
    child.assert_internal_consistency(&p).expect("internal consistency");
    child.assert_deadline_consistency(&p).expect("deadline consistency");
    Ok(child)
}

fn step(parent: &PalwChainStateV2, daa: u64, objects: &[PalwConsensusObjectV2]) -> Result<PalwChainStateV2, PalwStateV2Error> {
    step_with(parent, daa, objects, None)
}

fn bond(n: u64) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::BondRegistered {
        bond: bond_key(n),
        pubkey: vec![n as u8; 4],
        operator_pubkey: op_key(20 + n),
        collateral: 1_000_000_000,
        payout_payload: Hash64::from_u64_word(0x9A00 + n),
        capable_classes: Default::default(),
        signature: Vec::new(),
    }
}

/// The base class and three bonds at 100 — and the IR class, stood in by a carriage-less
/// registration of its `(class_id, artifact_root)` until the IR arm of the fold (F6 C(1)) lands.
fn registry_with(ir: &Ir) -> PalwChainStateV2 {
    let class = |class_id: Hash64, artifact_root: Hash64, share_permille: u16| PalwConsensusObjectV2::ClassRegistered {
        class_id,
        artifact_root,
        slash_value_per_pwu: 5,
        pwu_rule: PalwPwuRuleV2::MaxPerAttempt(160),
        initial_target: u128::MAX / 2,
        share_permille,
        activation_daa: 0,
        admission: None,
    };
    let objects = vec![class(h64(1), h64(11), 1000), bond(PRODUCER), bond(SEAT), bond(COLLUDER), class(ir.class_id, ir.root, 100)];
    step(&PalwChainStateV2::genesis(), 100, &objects).expect("the registry")
}

/// One claim: the anchor's job run by the producer's backend — honestly, or lying at `lie` — its
/// attempt envelope, and what the producer serves.
struct Claim {
    env: PalwAttemptEnvelopeV2,
    id: Hash64,
    job: PalwJobContextV2,
    prompt: Vec<usize>,
    material: Vec<u8>,
}

impl Claim {
    fn roots(&self) -> PalwClaimRootsV1 {
        PalwClaimRootsV1 {
            execution_root: self.env.attempt.execution_root,
            trace_root: self.env.attempt.trace_root,
            anchor: self.job.job_id,
            attempt_draw: Some(false),
            output_root: Some(self.env.attempt.output_root),
            job_pin: None,
        }
    }
}

fn produce(ir: &Ir, n: u64, lie: Option<u64>) -> Claim {
    let backend = ir.backend();
    let anchor = h64(0xA0_0000 + n);
    let (job, prompt) = backend.job_for_anchor(anchor).expect("the anchor's job");
    let job = kaspa_consensus_core::palw_attempt_v2::palw_attempt_job_v1(job, false);
    let run = match lie {
        None => backend.execute(&job, &prompt),
        Some(leaf) => backend.execute_with_injected_fault(&job, &prompt, leaf),
    }
    .expect("the producer's run");
    let network_domain = h64(999);
    let executor_bond = bond_key(PRODUCER).0;
    let env = PalwAttemptEnvelopeV2 {
        attempt: PalwAttemptUnsignedV2 {
            version: PALW_ATTEMPT_V2_VERSION,
            network_domain,
            challenge: challenge_v2(network_domain, h64(5 + n), 1_700 + n, n, ir.class_id, &executor_bond),
            class_id: ir.class_id,
            executor_bond,
            executor_pubkey: vec![7; 4],
            operator_id: palw_operator_id_v2(&op_key(20 + PRODUCER)),
            artifact_root: ir.root,
            trace_root: run.trace_root,
            output_root: run.output_root,
            pwu: 40,
            trace_manifest_root: run.trace_manifest_root,
            trace_chunk_count: run.trace_chunk_count,
            trace_retention_daa: 999_999,
            execution_root: run.execution_root,
        },
        signature: vec![0; 8],
    };
    let id = attempt_id_v2(&env.attempt);
    Claim { env, id, job, prompt, material: run.material }
}

/// The claim at `daa`, its panel (SEAT and COLLUDER) at `daa + 1`, licensed by both at `daa + 2`.
fn licensed(s: &PalwChainStateV2, claim: &Claim, daa: u64) -> PalwChainStateV2 {
    let s = step_with(s, daa, &[], Some(&claim.env)).expect("the claim");
    let seats = vec![
        PalwPanelSeatV2 { bond: bond_key(SEAT), operator_id: palw_operator_id_v2(&op_key(20 + SEAT)) },
        PalwPanelSeatV2 { bond: bond_key(COLLUDER), operator_id: palw_operator_id_v2(&op_key(20 + COLLUDER)) },
    ];
    let s = step(&s, daa + 1, &[PalwConsensusObjectV2::PanelBound { claim: claim.id, anchor: h64(77), seats }]).expect("the panel");
    let valid = |seat: u64| PalwSeatReceiptV2 {
        claim: Hash64::default(),
        verdict: PalwReceiptVerdictV2::Valid,
        seat_bond: bond_key(seat),
        signed_daa: 0,
        signature: Vec::new(),
    };
    step(&s, daa + 2, &[PalwConsensusObjectV2::ReceiptLicensed { claim: claim.id, receipts: vec![valid(SEAT), valid(COLLUDER)] }])
        .expect("the licence")
}

fn phase_of(s: &PalwChainStateV2, claim: &Hash64) -> PalwClaimPhaseV2 {
    s.claim(claim).expect("the claim").phase.clone()
}

fn duty_of(s: &PalwChainStateV2, bond: u64, sid: Hash64) -> Option<PalwCourtDutyV2> {
    palw_court_duties_v2(s, &[bond_key(bond)]).into_iter().find(|duty| duty.session_id == sid)
}

/// **The ladder, played by the node's own verbs**: the responder discloses its capture's prefix
/// state at every midpoint, the challenger compares its own (`agree` unless `contrarian`, a
/// challenger that disagrees with everything). Returns the state at `Terminal` and the next DAA.
fn play_ladder(
    ir: &Ir,
    mut s: PalwChainStateV2,
    sid: Hash64,
    responder_capture: &[u8],
    challenger_capture: &[u8],
    challenger: u64,
    contrarian: bool,
    mut daa: u64,
) -> (PalwChainStateV2, u64) {
    let backend = ir.backend();
    loop {
        let r = duty_of(&s, PRODUCER, sid).expect("the responder's duty");
        match r.turn {
            PalwBisectTurnV1::Terminal => return (s, daa),
            PalwBisectTurnV1::AwaitDisclosure => {
                let midpoint = r.midpoint.expect("a midpoint");
                let mid_state = backend.bisect_prefix_state(responder_capture, midpoint).expect("the responder's prefix state");
                let disclosure = PalwBisectDisclosureV1 {
                    version: PALW_BISECT_OBJECT_VERSION_V1,
                    session_id: sid,
                    round: r.round,
                    midpoint,
                    mid_state,
                };
                s = step(&s, daa, &[PalwConsensusObjectV2::CourtDisclosed { session_id: sid, disclosure, signature: Vec::new() }])
                    .expect("the disclosure");
            }
            PalwBisectTurnV1::AwaitVerdict => {
                let c = duty_of(&s, challenger, sid).expect("the challenger's duty");
                let (index, disclosed) = c.last_disclosure.expect("a disclosure to judge");
                let ours = backend.bisect_prefix_state(challenger_capture, index).expect("the challenger's prefix state");
                let verdict = PalwBisectVerdictV1 {
                    version: PALW_BISECT_OBJECT_VERSION_V1,
                    session_id: sid,
                    round: c.round,
                    agree: !contrarian && ours == disclosed,
                };
                s = step(&s, daa, &[PalwConsensusObjectV2::CourtVerdictPosted { session_id: sid, verdict, signature: Vec::new() }])
                    .expect("the verdict");
            }
            other => panic!("an unexpected turn {other:?}"),
        }
        daa += 1;
    }
}

/// The node's close for a narrowed session — the first candidate the adjudicator reads as this
/// party's win — and its verdict.
fn close(
    ir: &Ir,
    s: &PalwChainStateV2,
    sid: Hash64,
    accused: &[u8],
    own: Option<&[u8]>,
    index: u64,
    i_am_responder: bool,
) -> (&'static str, PalwCourtVerdictProofV2, PalwCourtVerdictV2) {
    let tir = ir.tir();
    let rules = tir.court_rules(&court());
    for (label, built) in palw_tir_close_candidates_v1(&tir, accused, own, index, &rules, !i_am_responder) {
        let proof = built.unwrap_or_else(|e| panic!("the {label} close builds: {e}"));
        assert!(proof.is_tir_v1(), "an IR close");
        let verdict =
            kaspa_consensus_core::palw_court_v2::adjudicate_court_close_v3(s, &sid, &proof, &court(), LADDER, FORM, false, false)
                .unwrap_or_else(|e| panic!("the {label} close adjudicates: {e}"));
        if palw_tir_close_is_mine_v1(verdict, i_am_responder) {
            return (label, proof, verdict);
        }
    }
    panic!("no IR close wins this party's side")
}

fn open_court(s: &PalwChainStateV2, claim: &Claim, challenger: u64, daa: u64) -> (PalwChainStateV2, Hash64) {
    let space = PalwBisectSpaceV1::StepLeaves;
    let sid = court_session_id_v2(&claim.id, &claim.env.attempt.trace_root, &bond_key(PRODUCER), &bond_key(challenger), space, LADDER);
    let opened = PalwConsensusObjectV2::CourtOpened {
        session_id: sid,
        claim: claim.id,
        challenger_bond: bond_key(challenger),
        space,
        space_size: LADDER,
        signature: Vec::new(),
    };
    (step(s, daa, &[opened]).expect("the court opens"), sid)
}

#[test]
fn an_ir_registration_is_refused_by_todays_fold_by_name() {
    use kaspa_consensus_core::config::params::{ForkActivation, palw_t12_shipped_params};
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
    use kaspa_consensus_core::palw_state_v2::PalwRegistrationTermsV2;
    use kaspa_consensus_core::palw_tir_v1::PalwTirFenceV1;
    let ir = ir_class("reg", false);
    let entry = misaka_palw_sdk::tir_registration::tir_entries_of_v1(ir.registry.holdings()).remove(0);
    let mut net = palw_t12_shipped_params();
    net.palw_tir_v1 = Some(PalwTirFenceV1::testnet12_v1(ForkActivation::new(0)));
    net.sync_palw_tir_v1();
    let PalwConsensusMode::ConsensusV2(bundle) = &net.palw_consensus_mode else { panic!("t12 is V2") };
    let terms = PalwRegistrationTermsV2 {
        min_grantable_share_permille: 1,
        slash_value_per_pwu: 5,
        initial_target: u128::MAX / 2,
        registered_class_ids: vec![h64(1)],
        registered_artifact_roots: vec![h64(11)],
        chain_certified_families: Vec::new(),
    };
    let object = misaka_palw_sdk::tir_registration::build_tir_registration_v1(
        &net,
        bundle,
        &entry,
        &terms,
        0,
        bond_key(PRODUCER),
        vec![0; 8],
        0,
    )
    .expect("the node builds the IR registration");
    assert!(matches!(&object, PalwConsensusObjectV2::ClassRegisteredTirV1 { class_id, artifact_root, .. }
        if *class_id == ir.class_id && *artifact_root == ir.root));
    // F6's consensus half (admission v10, the IR class record) has not landed: the fold refuses the
    // object by name. When it lands this assertion is replaced by the Candidate → Active walk.
    let base = step(&PalwChainStateV2::genesis(), 100, &[bond(PRODUCER)]).expect("a bond");
    match step(&base, 101, &[object]) {
        Err(PalwStateV2Error::TirRegistrationRefused(why)) => assert!(why.contains("F6"), "{why}"),
        other => panic!("today's fold refuses an IR registration by name, got {other:?}"),
    }
}

#[test]
fn an_honest_ir_claim_goes_final_and_a_false_accusation_is_defeated() {
    for tiled in [false, true] {
        let ir = ir_class(if tiled { "honest-tiled" } else { "honest-flat" }, tiled);
        let backend = ir.backend();
        let s = registry_with(&ir);
        let claim = produce(&ir, 1, None);
        // The seats' two checks: the served capture answers for the claim, and their own replay
        // reproduces its roots.
        assert_eq!(backend.verify_material(&claim.material, claim.roots()), PalwMaterialVerdictV1::Matches);
        let replay = backend.execute_for_verdict(&claim.job, &claim.prompt).expect("the seat's replay");
        assert_eq!((replay.execution_root, replay.trace_root), (claim.env.attempt.execution_root, claim.env.attempt.trace_root));
        let s = licensed(&s, &claim, 101);
        assert!(matches!(phase_of(&s, &claim.id), PalwClaimPhaseV2::ReceiptLicensed { .. }));

        // A contrarian challenger (COLLUDER) accuses the honest claim; the ladder narrows wherever its
        // disagreement leads, and the responder's TirCone close from its own capture acquits.
        let (s, sid) = open_court(&s, &claim, COLLUDER, 104);
        let (s, daa) = play_ladder(&ir, s, sid, &claim.material, &claim.material, COLLUDER, true, 105);
        let index = duty_of(&s, PRODUCER, sid).and_then(|d| d.terminal_index).expect("narrowed");
        let (label, proof, verdict) = close(&ir, &s, sid, &claim.material, None, index, true);
        assert_eq!((label, verdict), ("cone", PalwCourtVerdictV2::ChallengerDefeated), "tiled {tiled}");
        let s = step(&s, daa, &[PalwConsensusObjectV2::CourtClosed { session_id: sid, verdict, proof }]).expect("the acquittal");
        assert!(s.court_session(&sid).is_none(), "the session ends");

        // Past the window, final.
        let s = step(&s, daa + 200, &[]).expect("time passes");
        assert!(matches!(phase_of(&s, &claim.id), PalwClaimPhaseV2::Final { .. }), "tiled {tiled}: {:?}", phase_of(&s, &claim.id));
    }
}

#[test]
fn a_planted_lie_in_an_ir_claim_is_convicted_by_the_ir_court() {
    for tiled in [false, true] {
        let ir = ir_class(if tiled { "lie-tiled" } else { "lie-flat" }, tiled);
        let backend = ir.backend();
        let s = registry_with(&ir);
        // The honest run of the same job names the leaf the lie moves (a mid-job commit tile).
        let honest = produce(&ir, 2, None);
        let n = kaspa_consensus_core::palw_tir_step_v1::PalwTirStepSpaceV1::new(ir.tir().class())
            .unwrap()
            .leaf_count_capped(&honest.job, LADDER)
            .unwrap();
        let lie = n / 2;
        let liar = produce(&ir, 2, Some(lie));
        assert_ne!(liar.env.attempt.execution_root, honest.env.attempt.execution_root, "the lie is committed");
        // The served capture answers for the liar's roots (a challenger can find it), and the seat's
        // replay does not reproduce them.
        assert_eq!(backend.verify_material(&liar.material, liar.roots()), PalwMaterialVerdictV1::Matches);
        let replay = backend.execute_for_verdict(&liar.job, &liar.prompt).expect("the seat's replay");
        assert_ne!(replay.execution_root, liar.env.attempt.execution_root, "the replay refutes the claim");
        let s = licensed(&s, &liar, 101);
        let collateral = |s: &PalwChainStateV2, n: u64| s.bond(&bond_key(n)).expect("the bond").collateral;
        let before = collateral(&s, PRODUCER);

        // The seat opens; the ladder, from both parties' own captures, lands on the lie.
        let (s, sid) = open_court(&s, &liar, SEAT, 104);
        let (s, daa) = play_ladder(&ir, s, sid, &liar.material, &honest.material, SEAT, false, 105);
        let index = duty_of(&s, SEAT, sid).and_then(|d| d.terminal_index).expect("narrowed");
        assert_eq!(index, lie, "tiled {tiled}: the ladder lands on the planted lie");
        // The responder has no winning close; the challenger's is a TirCone the court convicts on.
        let tir = ir.tir();
        let rules = tir.court_rules(&court());
        for (label, built) in palw_tir_close_candidates_v1(&tir, &liar.material, None, index, &rules, false) {
            let verdict = kaspa_consensus_core::palw_court_v2::adjudicate_court_close_v3(
                &s,
                &sid,
                &built.unwrap(),
                &court(),
                LADDER,
                FORM,
                false,
                false,
            )
            .unwrap();
            assert!(!palw_tir_close_is_mine_v1(verdict, true), "the liar's {label} close does not acquit it");
        }
        let (label, proof, verdict) = close(&ir, &s, sid, &liar.material, Some(&honest.material), index, false);
        assert_eq!((label, verdict), ("cone", PalwCourtVerdictV2::ExecutorGuilty), "tiled {tiled}");
        let s = step(&s, daa, &[PalwConsensusObjectV2::CourtClosed { session_id: sid, verdict, proof }]).expect("the conviction");
        assert!(matches!(phase_of(&s, &liar.id), PalwClaimPhaseV2::Voided { .. }), "tiled {tiled}: {:?}", phase_of(&s, &liar.id));
        assert!(collateral(&s, PRODUCER) < before, "tiled {tiled}: the producer is charged");
    }
}
