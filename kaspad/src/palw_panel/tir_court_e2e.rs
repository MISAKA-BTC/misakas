//! **RFC-0002 Phase F (F6, node half), item 5: an IR class through the node's doors and the fold.**
//!
//! A tiny IR class — the golden `dense-gqa-2layer` program (every cone within the terminal
//! ceiling, so no dissection), its params, a declared layout — is written as a `PALWTIR1` container
//! and loaded through the SDK's one door into the node's backend registry. Everything the node does
//! with it goes through the doors kaspad uses: `resolve` (the trait door: job, execute, verify,
//! replay, bisection) and `resolve_tir_v1` (the IR close). The chain is the real transition
//! (`apply_palw_transition_v2_with_extras`) and the real adjudicator (`adjudicate_court_close_v3`).
//!
//! * **Registration, the walk to `Active`, and the one-move court** (`t12_walk`, on testnet-12's
//!   own fold with `palw_tir_v1` armed): the node's `ClassRegisteredTirV1` folds to a `Candidate`
//!   row; the admission jury is seated on the node's readiness V2 proofs; `Prefetching → Probation
//!   → ActiveLimited → Active` on IR probe claims the node produced going `Final`; an honest claim
//!   of the `Active` class goes `Final`; a planted lie licensed by its panel is convicted by the IR
//!   one-move court (`TirShardCourtAccused`, the node's case and pre-check, the gate's verdict) and
//!   voided. Every block is re-applied, reverted and reloaded through the import a node runs.
//! * **The one-move doors a bisection terminal does not reach** — a wrong generated token over an
//!   honest row (the decode-token door) and a trace row the steps did not compute (the logits door),
//!   each accused as the chain's one-move gate convicts it.
//! * **The bisection court** (below, on a ruleset without the held regime, which plays no bisection
//!   on testnet-12) — the class stood in by a carriage-less registration of its
//!   `(class_id, artifact_root)`:
//!   * **an honest claim goes `Final`** — produced from the anchor's job, the served capture answers
//!   for the claim's roots, the seats' replay reproduces them, the panel licenses it; a false
//!   accusation against it walks the ladder to a leaf the responder's `TirCone` close acquits
//!     (`ChallengerDefeated`); the claim is final past the window;
//!   * **a planted lie is convicted by the IR court** — one lane of one committed leaf moved and the
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
    ir_class_with(tag, tiled, court(), FORM)
}

/// [`ir_class`] served under a network's own court and prompt form.
fn ir_class_with(tag: &str, tiled: bool, court: PalwCourtParamsV2, form: PalwPromptIdsFormV1) -> Ir {
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
    let sdk = misaka_palw_sdk::PalwClassSdk::builtin_v1(court, form, NETWORK.to_vec());
    let holding = sdk.load_artifact(&path).expect("the IR lineage loads it by its magic");
    let registry = PalwBackendRegistry::new(court, form, vec![holding], NETWORK.to_vec());
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
fn an_honest_ir_claim_goes_final_and_a_false_accusation_is_defeated() {
    for tiled in [false, true] {
        let ir = ir_class(if tiled { "honest-tiled" } else { "honest-flat" }, tiled);
        let backend = ir.backend();
        let s = registry_with(&ir);
        let claim = produce(&ir, 1, None);
        // The producer's job is the chain's J5 context for the anchor (`palw_tir_attempt_v1`), drawn.
        {
            use kaspa_consensus_core::palw_tir_attempt_v1::{PalwTirJobFactsV1, palw_tir_attempt_context_v1};
            let tir = ir.tir();
            let facts = PalwTirJobFactsV1::of_class(tir.class(), ir.class_id).expect("the class decodes");
            let canonical = (claim.job.declared_prefill_tokens, claim.job.exact_decode_tokens);
            let expected = palw_tir_attempt_context_v1(&facts, &claim.job.job_id, canonical, claim.job.prompt_token_ids_hash);
            let drawn = kaspa_consensus_core::palw_attempt_v2::palw_attempt_job_v1(claim.job.clone(), true);
            assert_eq!(drawn.context_hash(), expected.context_hash(), "tiled {tiled}: the producer's job is J5's");
        }
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

        // The seat's one-move case names the same leaf the ladder will (the court that plays no
        // bisection is accused at it directly).
        {
            let tir = ir.tir();
            let rules = tir.court_rules(&court());
            let case =
                super::tir_court::palw_tir_one_move_case_v1(&tir, &liar.material, &honest.material, &rules).unwrap().expect("a case");
            assert_eq!(case.leaf, Some(lie), "tiled {tiled}: the one-move case names the lie");
            assert!(matches!(case.candidates.first(), Some(("cone", Ok(_)))));
            assert!(super::tir_court::palw_tir_one_move_case_v1(&tir, &honest.material, &honest.material, &rules).unwrap().is_none());
        }
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

/// **A claim whose producer computed honestly and committed a different answer** — the honest
/// capture edited by `edit` (its generated tokens or its trace rows), the trace and execution roots
/// re-derived over the edit, as a producer lying about its trace commits: every step leaf is the
/// honest execution's.
fn craft(ir: &Ir, honest: &Claim, edit: impl FnOnce(&mut misaka_palw_sdk::lineages::tir::TirCaptureV1)) -> Claim {
    use kaspa_consensus_core::palw_step_refute::{base0_logits_trace_root_v1, tiled_logits_scheme_id_v1, tiled_logits_trace_root_v1};
    let mut capture = misaka_palw_sdk::lineages::tir::TirCaptureV1::decode(&honest.material).expect("an IR capture");
    edit(&mut capture);
    let ctx = capture.binding.job_context.clone();
    let trace_root = if Hash64::from_bytes(ir.tir().space().program.logits_scheme_id) == tiled_logits_scheme_id_v1() {
        tiled_logits_trace_root_v1(&ctx, &capture.logits_rows, &capture.generated).expect("the rows build a trace")
    } else {
        base0_logits_trace_root_v1(&ctx, &capture.logits_rows, &capture.generated)
    };
    let b = &mut capture.binding;
    b.full_logits_trace_root = trace_root;
    b.committed_execution_root = kaspa_consensus_core::palw_tir_step_v1::palw_tir_execution_root_v1(
        &ctx.context_hash(),
        &trace_root,
        &ir.class_id,
        b.step_leaf_count,
        &b.step_merkle_root,
    );
    let mut env = honest.env.clone();
    env.attempt.trace_root = trace_root;
    env.attempt.execution_root = b.committed_execution_root;
    env.attempt.output_root = kaspa_consensus_core::palw_attempt_rules_v1::palw_attempt_output_root_v1(&ctx, &capture.generated);
    env.attempt.trace_manifest_root = kaspa_consensus_core::palw_attempt_v2::attempt_trace_manifest_root_v1(trace_root, 1);
    let id = attempt_id_v2(&env.attempt);
    Claim { env, id, job: honest.job.clone(), prompt: honest.prompt.clone(), material: capture.encode() }
}

/// **The one-move doors a bisection terminal does not reach** (F6, the node's one-move case): a
/// wrong answer over honest arithmetic. Every step leaf of these claims is the honest execution's,
/// so no step leaf parts from a challenger's own run; the case finds the lie where it is —
///
/// * a generated token that is not the greedy selection of its (honest) row: the decode-token door,
///   the challenger's own token the lane that beats it;
/// * a committed trace row that is not the one the steps computed (the selection unchanged): the
///   logits door over the step tile holding the first lane that differs;
///
/// and the accusation the node would file is the one the chain's one-move gate derives
/// `ExecutorGuilty` from (`palw_tir_one_move_verdict_v1` on the claim's state). An accusation of an
/// honest claim finds nothing to file.
#[test]
fn a_wrong_answer_over_honest_arithmetic_is_accused_in_one_move() {
    use super::tir_court::{palw_tir_one_move_accusation_to_file_v1, palw_tir_one_move_case_v1};
    use kaspa_consensus_core::palw_producer_v2::palw_disputable_claims_v2;
    use kaspa_consensus_core::palw_tir_one_move_v1::{palw_tir_one_move_shape_v1, palw_tir_one_move_verdict_v1};
    for tiled in [false, true] {
        let ir = ir_class(if tiled { "om-tiled" } else { "om-flat" }, tiled);
        let tir = ir.tir();
        let rules = tir.court_rules(&court());
        let s = registry_with(&ir);
        let honest = produce(&ir, 5, None);
        let greedy = misaka_palw_sdk::lineages::tir::TirCaptureV1::decode(&honest.material).unwrap().generated[0];
        let lies: [(&str, Box<dyn Fn(&mut misaka_palw_sdk::lineages::tir::TirCaptureV1)>); 2] = [
            ("decode token", Box::new(|c| c.generated[0] = (c.generated[0] + 1) % c.logits_rows[0].len() as u32)),
            (
                "logits",
                Box::new(|c| {
                    let lane = if c.generated[0] == 0 { 1 } else { 0 };
                    c.logits_rows[0][lane] = c.logits_rows[0][lane].wrapping_sub(1);
                }),
            ),
        ];
        for (door, edit) in lies {
            let liar = craft(&ir, &honest, edit);
            assert_ne!(liar.env.attempt.execution_root, honest.env.attempt.execution_root, "{door}: the lie is committed");
            let s = licensed(&s, &liar, 101);
            let case = palw_tir_one_move_case_v1(&tir, &liar.material, &honest.material, &rules).unwrap().expect("a case");
            assert_eq!(case.leaf, None, "tiled {tiled}, {door}: every step leaf is the honest execution's");
            assert_eq!(case.row.is_some(), door == "decode token", "tiled {tiled}, {door}: the selection moved or it did not");
            let target =
                palw_disputable_claims_v2(&s, &[bond_key(SEAT)]).into_iter().find(|t| t.claim_id == liar.id).expect("disputable");
            let (label, mut accusation) =
                palw_tir_one_move_accusation_to_file_v1(case.candidates, &target, bond_key(SEAT), &court(), LADDER, FORM)
                    .unwrap_or_else(|| panic!("tiled {tiled}, {door}: a close convicts"));
            assert_eq!(label, door, "tiled {tiled}");
            accusation.signature = vec![3; 16];
            palw_tir_one_move_shape_v1(&accusation).expect("the shape");
            let claim = s.claim(&liar.id).expect("the claim");
            assert_eq!(
                palw_tir_one_move_verdict_v1(&s, claim, &accusation, &court(), LADDER, FORM),
                Ok(PalwCourtVerdictV2::ExecutorGuilty),
                "tiled {tiled}, {door}: the chain derives the node's verdict"
            );
        }
        // The honest claim: no case, and an accusation built against it anyway finds nothing to file.
        assert!(palw_tir_one_move_case_v1(&tir, &honest.material, &honest.material, &rules).unwrap().is_none());
        let s = licensed(&s, &honest, 101);
        let target = palw_disputable_claims_v2(&s, &[bond_key(SEAT)]).into_iter().find(|t| t.claim_id == honest.id).unwrap();
        let wrong_beat = (greedy + 1) % 4;
        let against = vec![
            ("cone", tir.cone_close(&honest.material, 3, &rules)),
            ("decode token", tir.decode_token_close(&honest.material, 0, wrong_beat)),
        ];
        assert!(
            palw_tir_one_move_accusation_to_file_v1(against, &target, bond_key(SEAT), &court(), LADDER, FORM).is_none(),
            "tiled {tiled}: nothing convicts an honest claim"
        );
    }
}

/// **An IR claim answers its data-availability demands in the IR form** (F6 D): the node's own
/// answering path (`palw_da_claim_answers_v1`: the kept capture verified against the claim, then each
/// unit) discloses a trace event of an IR claim as a `TirEvent` — the row's tile opened under the
/// class's scheme, or every row at once under the flat scheme, or `OutOfRange` for an event outside
/// the run — and the chain's checker (`check_tir_trace_event_disclosure_v1`) takes every answer as
/// refuting the accusation. A held unit is the legacy families' and is refused by name.
#[test]
fn an_ir_claims_trace_events_are_answered_in_the_ir_form() {
    use kaspa_consensus_core::palw_da_rcore_v1::{PalwDaAnswerV1, PalwDaUnitV1};
    use kaspa_consensus_core::palw_held_da_v1::PalwHeldMissingV1;
    use kaspa_consensus_core::palw_tir_court_v1::{PalwTirTraceEventDisclosureV1, check_tir_trace_event_disclosure_v1};
    for tiled in [false, true] {
        let ir = ir_class(if tiled { "da-tiled" } else { "da-flat" }, tiled);
        let backend = ir.backend();
        let claim = produce(&ir, 3, None);
        let facts = super::PalwDaClaimFactsV1 {
            claim_id: claim.id,
            class_id: ir.class_id,
            executor_bond: bond_key(PRODUCER),
            execution_root: claim.env.attempt.execution_root,
            trace_root: claim.env.attempt.trace_root,
            work_leaves: 0,
            form: FORM,
            lane: super::PalwDaLaneV1::Attempt {
                anchor: claim.job.job_id,
                attempt_draw: Some(false),
                job: Some((claim.job.clone(), claim.prompt.clone())),
            },
            job_pin: None,
        };
        let rows = claim.job.exact_decode_tokens;
        let units = vec![
            PalwDaUnitV1::Event { row: rows - 1, tile: 0 },
            PalwDaUnitV1::Event { row: 0, tile: 0 },
            PalwDaUnitV1::Event { row: rows + 3, tile: 0 },
            PalwDaUnitV1::Event { row: 0, tile: 200 },
        ];
        let answered =
            super::palw_da_claim_answers_v1(backend.as_ref(), &facts, vec![claim.material.clone()], |_| {}, &units, rows, false)
                .expect("the kept capture answers");
        assert!(!answered.remade, "answered from the kept capture");
        let (mut checked, mut out_of_range) = (0, 0);
        for (unit, answer) in units.iter().zip(answered.answers) {
            let PalwDaUnitV1::Event { row, tile } = *unit else { unreachable!() };
            let Some(answer) = answer else {
                assert!(!tiled, "only a flat answer covers the other in-run events");
                continue;
            };
            let Ok(PalwDaAnswerV1::TirEvent(disclosure)) = answer else { panic!("tiled {tiled}: an IR event answer, got {answer:?}") };
            if matches!(*disclosure, PalwTirTraceEventDisclosureV1::OutOfRange { .. }) {
                out_of_range += 1;
            } else {
                assert_eq!(disclosure.is_flat(), !tiled, "the class's scheme");
            }
            check_tir_trace_event_disclosure_v1(
                claim.env.attempt.trace_root,
                claim.env.attempt.execution_root,
                row,
                tile,
                &disclosure,
                LADDER,
            )
            .unwrap_or_else(|e| panic!("tiled {tiled}: event ({row}, {tile}) is answered: {e}"));
            checked += 1;
        }
        assert!(checked >= 2 && out_of_range >= 1, "tiled {tiled}: {checked} checked, {out_of_range} out of range");
        // A held unit is not an IR claim's to answer.
        let held = [PalwDaUnitV1::Held(PalwHeldMissingV1::StepLeaf { leaf: 0 })];
        let answered =
            super::palw_da_claim_answers_v1(backend.as_ref(), &facts, vec![claim.material.clone()], |_| {}, &held, rows, false)
                .expect("the kept capture");
        assert!(matches!(&answered.answers[0], Some(Err(why)) if why.contains("held unit")), "{:?}", answered.answers[0]);
    }
}

/// **The whole walk on testnet-12's own fold** (F6 C(1)–C(3) and the one-move court): the IR class
/// registered by the node's own object, its lifecycle row walked `Candidate → Prefetching →
/// Probation → ActiveLimited → Active` by the chain's registry — the admission jury seated on the
/// node's readiness V2 proofs, the probation passed by IR claims the node produced going `Final` —
/// an honest claim of the `Active` class going `Final`, and a planted lie convicted in one move.
/// testnet-12's fold harness (`consensus/core/tests/rcore_common.rs`: the real fold with the
/// processor's extras, every block re-applied, reverted and reloaded).
#[path = "../../../consensus/core/tests/rcore_common.rs"]
#[allow(dead_code, unused_imports, clippy::all)]
mod rcore;

mod t12_walk {
    use super::Ir;
    use kaspa_consensus_core::config::params::{ForkActivation, Params, palw_t12_shipped_params};
    use kaspa_consensus_core::palw_attempt_v2::{
        PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, attempt_id_v2, challenge_v2, execution_anchor_v3,
        execution_commitment_v3,
    };
    use kaspa_consensus_core::palw_backend::{PalwClaimRootsV1, PalwExecutionBackendV1, PalwMaterialVerdictV1};
    use kaspa_consensus_core::palw_model_registry_v1::{
        PALW_READINESS_V2_BUDGET_BYTES_V1, PalwModelLifecycleV1, palw_readiness_v2_challenge_seed_v1, palw_readiness_v2_draw_v1,
    };
    use kaspa_consensus_core::palw_state_v2::{
        PalwBlockWorkV3, PalwBondKeyV2, PalwClaimPhaseV2, PalwConsensusObjectV2, PalwRegistrationTermsV2,
    };
    use kaspa_consensus_core::palw_tir_v1::PalwTirFenceV1;
    use kaspa_hashes::Hash64;

    use super::rcore;

    /// The height testnet-12's IR fence arms at here (Phase F's own fold test's).
    const AT: u64 = 1_100;

    fn armed() -> Params {
        let mut p = palw_t12_shipped_params();
        p.palw_tir_v1 = Some(PalwTirFenceV1::testnet12_v1(ForkActivation::new(AT)));
        p.sync_palw_tir_v1();
        p
    }

    /// The registry's span on `c`'s ruleset at `daa`.
    fn span_daa(c: &rcore::Chain, daa: u64) -> u64 {
        rcore::registry_fold(&c.p, daa).expect("the registry governs").span_daa
    }

    fn lifecycle(c: &rcore::Chain, class: &Hash64) -> Option<PalwModelLifecycleV1> {
        c.s.model_lifecycle(class).map(|row| row.state)
    }

    /// **One block on `c`, checked as the harness checks it — with the PRODUCTION import.** The
    /// fold at `daa` with the processor's extras; the delta re-applied and reverted; and the carriage
    /// reloaded under its root by `into_state_v3` with testnet-12's own `palw_uncertified_weightless`
    /// and canonical-work height, the import every node runs (the harness's `step_at` reloads with
    /// the fixtures' `into_state`, whose weight rule a weightless class's `Final` does not obey).
    fn step(c: &mut rcore::Chain, daa: u64, objects: &[PalwConsensusObjectV2], work: PalwBlockWorkV3<'_>, key: Hash64, subsidy: u64) {
        use kaspa_consensus_core::palw_state_v2::{PalwStateCarriageV2, apply_delta_v2, revert_delta_v2};
        assert!(daa > c.daa, "DAA moves forward");
        let at = rcore::ctx(0xCA_0000 + daa, daa, daa, subsidy);
        let parent = c.s.clone();
        let (child, delta, skips) =
            c.try_fold(&parent, &at, objects, work, key).unwrap_or_else(|e| panic!("the block at DAA {daa} folds: {e}"));
        assert!(skips.is_empty(), "nothing skipped at {daa}: {skips:?}");
        assert_eq!(apply_delta_v2(&parent, &delta, &c.sp).expect("re-applies"), child, "DAA {daa}: the delta is the transition");
        assert_eq!(revert_delta_v2(&child, &delta, &c.sp).expect("reverts"), parent, "DAA {daa}: the delta reverts");
        let weightless = c.p.palw_uncertified_weightless.is_some_and(|f| f.is_active(daa));
        let reloaded = PalwStateCarriageV2::from_state(&child)
            .into_state_v3(&c.sp, Some(child.state_root()), weightless, c.p.palw_canonical_work_daa())
            .unwrap_or_else(|e| panic!("DAA {daa}: the carriage reloads as a node imports it: {e}"));
        assert_eq!(reloaded, child, "DAA {daa}: reload is the state");
        c.s = child;
        c.daa = daa;
    }

    fn empty(c: &mut rcore::Chain, daa: u64) {
        step(c, daa, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    }

    /// A floor attempt by the floor's producer in its own block at `daa` (the jury's seed anchor).
    fn floor_claim(c: &mut rcore::Chain, seed: u64, daa: u64) {
        let (floor, _, _, _) = rcore::genesis_classes(&c.p)[0];
        let (bond, pubkey, operator) = rcore::floor_producer(&c.p);
        let pwu = c.floor_pwu(daa);
        let (env, key, id) = rcore::junk_attempt(floor, bond, pubkey, &operator, pwu, seed, 0x10C0 + seed);
        step(c, daa, &[], PalwBlockWorkV3::Attempt(&env), key, rcore::T12_BLOCK_SUBSIDY_SOMPI);
        assert!(c.s.claim(&id).is_some(), "the floor attempt is accepted");
    }

    /// `claim` bound to `seats` in the next block; returns its DAA.
    fn bind(c: &mut rcore::Chain, claim: Hash64, seats: &[(PalwBondKeyV2, Hash64)]) -> u64 {
        let daa = c.daa + 1;
        let anchor = rcore::h(0xAC_0000 + daa);
        step(
            c,
            daa,
            &[PalwConsensusObjectV2::PanelBound { claim, anchor, seats: rcore::seats_of(seats) }],
            PalwBlockWorkV3::None,
            Hash64::default(),
            0,
        );
        daa
    }

    /// **A seat's readiness V2 proof, built as the node's readiness duty builds it** — the challenge
    /// of `(class, bond, span)` drawn over the IR inventory, its prefix opened within the carrier's
    /// budget from the backend's readiness material, one multiproof.
    fn readiness(backend: &dyn PalwExecutionBackendV1, class: Hash64, bond: PalwBondKeyV2, span: u64) -> PalwConsensusObjectV2 {
        let (root, leaf_count) = backend.artifact_root_and_leaf_count().expect("the inventory");
        let bond_bytes = borsh::to_vec(&bond).expect("a bond key");
        let draw = palw_readiness_v2_draw_v1(&palw_readiness_v2_challenge_seed_v1(&class, &bond_bytes, span), leaf_count);
        let (_, leaves, drawn) = backend.artifact_readiness_material(&draw).expect("the drawn leaves open");
        let mut opened = Vec::new();
        let mut bytes = 0usize;
        for (index, operand) in drawn {
            if bytes >= PALW_READINESS_V2_BUDGET_BYTES_V1 {
                break;
            }
            bytes += operand.bytes.len();
            opened.push((index, operand));
        }
        opened.sort_by_key(|(index, _)| *index);
        let proof = kaspa_consensus_core::palw_artifact::palw_artifact_multiproof_v1(&leaves, &opened).expect("a multiproof");
        kaspa_consensus_core::palw_artifact::verify_artifact_multiproof_v1(&proof, root).expect("it opens the class root");
        PalwConsensusObjectV2::SeatReadinessProvedV2 { bond, class_id: class, span, proof: Box::new(proof), signature: Vec::new() }
    }

    /// Every genesis bond but the registrant proves readiness for `span`, in one block at `daa`.
    fn prove_all(c: &mut rcore::Chain, ir: &Ir, registrant: PalwBondKeyV2, span: u64, daa: u64) {
        let backend = ir.backend();
        let proofs: Vec<_> = rcore::honest(&c.p)
            .into_iter()
            .filter(|k| *k != registrant)
            .map(|k| readiness(backend.as_ref(), ir.class_id, k, span))
            .collect();
        step(c, daa, &proofs, PalwBlockWorkV3::None, Hash64::default(), 0);
    }

    /// The pwu an IR attempt of `class` carries at `daa`: the derived draw of its registry row at
    /// the class's effective target (what admission accepts, as for any model class).
    fn ir_pwu(c: &rcore::Chain, class: &Hash64, daa: u64) -> u64 {
        let per_draw = c.s.palw_canonical_per_draw_v1(class, daa, Some(0)).expect("the class's registry row");
        let target =
            kaspa_consensus_core::palw_admission_v2::palw_effective_class_target_v1(&c.s, &c.sp, class, None).expect("a target");
        kaspa_consensus_core::palw_admission_v2::palw_attempt_derived_pwu_v1(target, per_draw)
    }

    /// **An IR attempt by rich bond `n`**: the job its anchor names, run by the node's IR backend,
    /// committed in the attempt envelope; accepted in its own block at `daa` (or `None` where the
    /// class gate refuses it at that height).
    fn ir_claim(c: &mut rcore::Chain, ir: &Ir, n: u64, seed: u64, daa: u64) -> Option<(Hash64, Vec<u8>, PalwClaimRootsV1)> {
        ir_claim_lying(c, ir, n, seed, daa, None)
    }

    /// [`ir_claim`], the run lying at step leaf `lie` (one lane moved, the commitment re-derived
    /// over it) where one is named.
    fn ir_claim_lying(
        c: &mut rcore::Chain,
        ir: &Ir,
        n: u64,
        seed: u64,
        daa: u64,
        lie: Option<u64>,
    ) -> Option<(Hash64, Vec<u8>, PalwClaimRootsV1)> {
        let backend = ir.backend();
        let bond = rcore::bond_key(n);
        let (pre_pow, nonce) = (0x7C0_0000 + seed, 7u64);
        let anchor = backend
            .job_anchor_v1(
                rcore::h(rcore::NET),
                rcore::h(pre_pow),
                ir.class_id,
                &bond.0,
                kaspa_consensus_core::palw_attempt_v2::palw_nonce_bucket_v1(nonce),
            )
            .expect("an anchor");
        let (job, prompt) = backend.job_for_anchor(anchor).expect("the anchor's job");
        let job = kaspa_consensus_core::palw_attempt_v2::palw_attempt_job_v1(job, false);
        let run = match lie {
            None => backend.execute(&job, &prompt),
            Some(leaf) => backend.execute_with_injected_fault(&job, &prompt, leaf),
        }
        .expect("the IR run");
        let attempt = PalwAttemptUnsignedV2 {
            version: PALW_ATTEMPT_V2_VERSION,
            network_domain: rcore::h(rcore::NET),
            challenge: challenge_v2(rcore::h(rcore::NET), rcore::h(pre_pow), 1_700_000_000 + seed, nonce, ir.class_id, &bond.0),
            class_id: ir.class_id,
            executor_bond: bond.0,
            executor_pubkey: rcore::pubkey_of(n),
            operator_id: kaspa_consensus_core::palw_state_v2::palw_operator_id_v2(&rcore::operator_pubkey_of(n)),
            artifact_root: ir.root,
            trace_root: run.trace_root,
            output_root: run.output_root,
            pwu: ir_pwu(c, &ir.class_id, daa),
            trace_manifest_root: run.trace_manifest_root,
            trace_chunk_count: run.trace_chunk_count,
            trace_retention_daa: 999_999,
            execution_root: run.execution_root,
        };
        let env =
            PalwAttemptEnvelopeV2 { attempt, signature: vec![0u8; kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN] };
        let key = execution_commitment_v3(
            &env.attempt,
            execution_anchor_v3(rcore::h(rcore::NET), rcore::h(pre_pow), ir.class_id, &bond.0, nonce),
        );
        let id = attempt_id_v2(&env.attempt);
        let at = rcore::ctx(0xCA_0000 + daa, daa, daa, rcore::T12_BLOCK_SUBSIDY_SOMPI);
        if let Err(e) = c.try_fold(&c.s.clone(), &at, &[], PalwBlockWorkV3::Attempt(&env), key) {
            eprintln!("DAA {daa}: the class gate refuses the IR attempt: {e}");
            return None;
        }
        step(c, daa, &[], PalwBlockWorkV3::Attempt(&env), key, rcore::T12_BLOCK_SUBSIDY_SOMPI);
        let roots = PalwClaimRootsV1 {
            execution_root: run.execution_root,
            trace_root: run.trace_root,
            anchor,
            attempt_draw: Some(false),
            output_root: Some(run.output_root),
            job_pin: None,
        };
        Some((id, run.material, roots))
    }

    #[test]
    fn an_ir_class_walks_to_active_on_testnet_12_its_claims_go_final_and_a_lie_is_convicted_in_one_move() {
        let mut c = rcore::Chain::new(armed());
        c.room = true;
        let bundle = rcore::bundle(&c.p);
        let ir = super::ir_class_with("t12-walk", false, bundle.court, c.p.palw_prompt_ids_form_v1());
        let backend = ir.backend();
        let (registrant, _, _) = rcore::floor_producer(&c.p);

        // 1. The node's registration (SDK builder, admission v10 at the gate), folded at the fence.
        let entry = misaka_palw_sdk::tir_registration::tir_entries_of_v1(ir.registry.holdings()).remove(0);
        let (floor, _, target, slash) = rcore::genesis_classes(&c.p)[0];
        let terms = PalwRegistrationTermsV2 {
            min_grantable_share_permille: 0,
            slash_value_per_pwu: slash,
            initial_target: c.s.class_target(&floor).map(|t| t.target).unwrap_or(target),
            registered_class_ids: rcore::genesis_classes(&c.p).iter().map(|g| g.0).collect(),
            registered_artifact_roots: Vec::new(),
            chain_certified_families: Vec::new(),
        };
        let object = misaka_palw_sdk::tir_registration::build_tir_registration_v1(
            &c.p,
            &bundle,
            &entry,
            &terms,
            AT,
            registrant,
            vec![9; 16],
            AT,
        )
        .expect("admission v10 admits the tiny class");
        step(&mut c, AT, &[object], PalwBlockWorkV3::None, Hash64::default(), 0);
        assert!(c.s.tir_class_v1(&ir.class_id).is_some(), "the tir_classes row");
        empty(&mut c, AT + 1);
        assert_eq!(lifecycle(&c, &ir.class_id), Some(PalwModelLifecycleV1::Candidate), "a bought class opens Candidate");

        // 2. The admission jury: every other genesis bond proves possession two spans before the
        //    class's staggered audit; a floor attempt in the span before is the jury's anchor.
        let span = span_daa(&c, c.daa);
        let period = kaspa_consensus_core::palw_model_registry_v1::palw_admission_audit_period_spans_v2(
            c.sp.epoch_length(),
            span,
            c.p.palw_admission_audit_period_daa,
        );
        let now_span = c.daa / span;
        let audit = (now_span + 3..)
            .find(|s| kaspa_consensus_core::palw_activation_pool_v1::palw_admission_audit_due_staggered_v1(&ir.class_id, *s, period))
            .unwrap();
        prove_all(&mut c, &ir, registrant, audit - 2, (audit - 2) * span);
        floor_claim(&mut c, 0x7A11, (audit - 1) * span);
        empty(&mut c, audit * span);
        assert_eq!(lifecycle(&c, &ir.class_id), Some(PalwModelLifecycleV1::Prefetching), "the jury is seated");

        // 3. Ready seats: Prefetching → Probation.
        prove_all(&mut c, &ir, registrant, audit + 1, (audit + 1) * span);
        empty(&mut c, (audit + 2) * span);
        assert!(
            matches!(lifecycle(&c, &ir.class_id), Some(PalwModelLifecycleV1::Probation { .. })),
            "{:?}",
            lifecycle(&c, &ir.class_id)
        );

        // 4. Probation: the class's probes — IR attempts by a rich producer, each served (the seats'
        //    material check), bound to a panel of genesis seats, licensed, and Final.
        let next = c.daa + 1;
        step(&mut c, next, &[rcore::bond_obj(1, rcore::RICH)], PalwBlockWorkV3::None, Hash64::default(), 0);
        let seats = rcore::honest_seats(&c.p, bundle.panel.seat_count() as usize);
        let probes = rcore::registry_fold(&c.p, c.daa).expect("the registry").globals.probation_claims as usize;
        let mut claims: Vec<Hash64> = Vec::new();
        let mut seed = 0u64;
        while claims.len() < probes {
            keep_ready(&mut c, &ir, registrant);
            seed += 1;
            let daa = c.daa + 1;
            match ir_claim(&mut c, &ir, 1, seed, daa) {
                Some((id, material, roots)) => {
                    assert_eq!(backend.verify_material(&material, roots), PalwMaterialVerdictV1::Matches, "the served capture");
                    let replay = backend.execute_for_verdict(&job_of(&material), &prompt_of(&material)).expect("the seat's replay");
                    assert_eq!(replay.execution_root, roots.execution_root, "the seats' replay reproduces the claim");
                    let bound = bind(&mut c, id, &seats);
                    let receipts = seats.iter().map(|(k, _)| rcore::valid(id, *k, bound)).collect();
                    step(
                        &mut c,
                        bound + 1,
                        &[PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts }],
                        PalwBlockWorkV3::None,
                        Hash64::default(),
                        0,
                    );
                    claims.push(id);
                }
                None => empty(&mut c, daa),
            }
        }
        let last = claims.iter().map(|id| c.s.deadline_of(id).expect("a licensed claim's Final deadline")).max().unwrap();
        advance_to(&mut c, &ir, registrant, last + 1);
        for id in &claims {
            assert!(matches!(c.claim(id).phase, PalwClaimPhaseV2::Final { .. }), "probe {id} is Final: {:?}", c.claim(id).phase);
        }
        eprintln!("walk: {:?} at DAA {} after {probes} probe Finals", lifecycle(&c, &ir.class_id), c.daa);

        // 5. ActiveLimited, then stable spans to Active.
        let span = span_daa(&c, c.daa);
        let mut guard = 0;
        while !matches!(lifecycle(&c, &ir.class_id), Some(PalwModelLifecycleV1::Active)) {
            guard += 1;
            assert!(guard < 200, "the walk stalls at {:?}", lifecycle(&c, &ir.class_id));
            let next = (c.daa / span + 1) * span;
            advance_to(&mut c, &ir, registrant, next);
        }
        eprintln!("walk: Active at DAA {}", c.daa);

        // 6. An honest claim of the Active class goes Final.
        keep_ready(&mut c, &ir, registrant);
        let (id, material, roots) = loop {
            seed += 1;
            let daa = c.daa + 1;
            if let Some(claim) = ir_claim(&mut c, &ir, 1, seed, daa) {
                break claim;
            }
            empty(&mut c, daa);
        };
        assert_eq!(backend.verify_material(&material, roots), PalwMaterialVerdictV1::Matches);
        let bound = bind(&mut c, id, &seats);
        let receipts = seats.iter().map(|(k, _)| rcore::valid(id, *k, bound)).collect();
        step(
            &mut c,
            bound + 1,
            &[PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts }],
            PalwBlockWorkV3::None,
            Hash64::default(),
            0,
        );
        let deadline = c.s.deadline_of(&id).expect("its Final deadline");
        advance_to(&mut c, &ir, registrant, deadline + 1);
        assert!(matches!(c.claim(&id).phase, PalwClaimPhaseV2::Final { .. }), "the Active class's claim is Final");
        assert_eq!(lifecycle(&c, &ir.class_id), Some(PalwModelLifecycleV1::Active));

        // 7. A planted lie in a claim of the Active class — licensed by a panel that signed it
        //    anyway — is convicted by the IR court in one move (the held regime plays no bisection:
        //    `CourtOpened` is refused on testnet-12). The challenger's steps are the node's
        //    (`tir_one_move_pass_v1`): the claim as `palw_disputable_claims_v2` lists it, its own run
        //    of the job the claim's anchor names, the one-move case against the served capture, the
        //    first close its pre-check convicts on; the chain's gate derives the same verdict and the
        //    fold voids the claim and charges its producer.
        let honest_leaves = misaka_palw_sdk::lineages::tir::TirCaptureV1::decode(&material).unwrap().binding.step_leaf_count;
        let lie = honest_leaves / 2;
        keep_ready(&mut c, &ir, registrant);
        let (id, material, roots) = loop {
            seed += 1;
            let daa = c.daa + 1;
            if let Some(claim) = ir_claim_lying(&mut c, &ir, 1, seed, daa, Some(lie)) {
                break claim;
            }
            empty(&mut c, daa);
        };
        assert_eq!(backend.verify_material(&material, roots), PalwMaterialVerdictV1::Matches, "the lie answers for its own roots");
        let bound = bind(&mut c, id, &seats);
        let receipts = seats.iter().map(|(k, _)| rcore::valid(id, *k, bound)).collect();
        step(
            &mut c,
            bound + 1,
            &[PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts }],
            PalwBlockWorkV3::None,
            Hash64::default(),
            0,
        );
        assert!(matches!(c.claim(&id).phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "the lie is licensed");
        assert!(c.p.palw_held_context_active_at(c.daa), "testnet-12's court plays no bisection");

        let accuser = seats[0].0;
        let target = kaspa_consensus_core::palw_producer_v2::palw_disputable_claims_v2(&c.s, &[accuser])
            .into_iter()
            .find(|t| t.claim_id == id)
            .expect("the licensed lie is disputable");
        let (job, prompt) = backend.job_for_anchor(roots.anchor).expect("the anchor's job");
        let job = kaspa_consensus_core::palw_attempt_v2::palw_attempt_job_v1(job, false);
        let own = backend.execute(&job, &prompt).expect("the challenger's own run");
        assert_ne!(own.execution_root, target.execution_root, "the challenger does not reproduce the claim");
        let court = bundle.court;
        let ladder = c.s.class_step_ladder_v1(
            &ir.class_id,
            kaspa_consensus_core::palw_court_v2::palw_refutation_leaf_cap_v2(
                &court,
                c.p.palw_court_ladder.is_some_and(|f| f.is_active(c.daa)),
            ),
        );
        let form = c.p.palw_prompt_ids_form_v1();
        let tir = ir.tir();
        let mut rules = tir.court_rules(&court);
        rules.max_step_leaf_count = ladder;
        let case = super::super::tir_court::palw_tir_one_move_case_v1(&tir, &material, &own.material, &rules)
            .expect("the case builds")
            .expect("the lie is a case");
        assert_eq!(case.leaf, Some(lie), "the case names the planted leaf");
        let (label, mut accusation) =
            super::super::tir_court::palw_tir_one_move_accusation_to_file_v1(case.candidates, &target, accuser, &court, ladder, form)
                .expect("a close convicts");
        accusation.signature = vec![5; 16];
        kaspa_consensus_core::palw_tir_one_move_v1::palw_tir_one_move_shape_v1(&accusation).expect("the shape");
        assert_eq!(
            kaspa_consensus_core::palw_tir_one_move_v1::palw_tir_one_move_verdict_v1(
                &c.s,
                c.s.claim(&id).expect("the claim"),
                &accusation,
                &court,
                ladder,
                form
            ),
            Ok(kaspa_consensus_core::palw_state_v2::PalwCourtVerdictV2::ExecutorGuilty),
            "the gate derives the node's verdict"
        );
        let object = PalwConsensusObjectV2::TirShardCourtAccused { accusation: Box::new(accusation) };
        kaspa_consensus_core::palw_lifecycle_objects_v2::palw_lifecycle_object_may_ride_v2(&object).expect("it rides a carrier");
        let producer = rcore::bond_key(1);
        let before = c.s.bond(&producer).expect("the producer").collateral;
        let next = c.daa + 1;
        step(&mut c, next, &[object], PalwBlockWorkV3::None, Hash64::default(), 0);
        assert!(matches!(c.claim(&id).phase, PalwClaimPhaseV2::Voided { .. }), "the lie is voided: {:?}", c.claim(&id).phase);
        let after = c.s.bond(&producer).expect("the producer").collateral;
        eprintln!("walk: the lie is convicted in one move ({label} close at leaf {lie}); producer collateral {before} -> {after}");
        assert!(after < before, "the producer is charged");
    }

    /// **A Hugging Face model through the operator's path, to a registration on testnet-12's fold**
    /// (the operator surface, RFC-0002 Phase F): the tiny `llama` fixture's config is ADMISSIBLE by
    /// `check-architecture`'s IR mode; the converter's library (`fidelity::prepare`, calibration,
    /// `materialise`, `artifact::write` — what `palw-tir-fidelity --artifact-out` runs, with the
    /// class's window) writes its integer artifact; `declare-layout` makes it a class admission v10
    /// admits; the SDK's IR lineage loads it, its backend runs the canonical job a seat checks; and
    /// the node's registration folds to a `Candidate` row.
    #[test]
    fn a_tiny_hf_model_is_checked_lowered_declared_and_registered_on_testnet_12() {
        use misaka_palw_tir_lower::float_ref::ParamStore;
        use misaka_palw_tir_lower::float_ref::stream::Resident;
        use misaka_palw_tir_lower::lower::{LowerOpts, materialise};
        use misaka_palw_tir_lower::quant::QuantPolicy;
        use misaka_palw_tir_lower::weights::Checkpoint;
        use misaka_palw_tir_lower::{artifact, fidelity};
        const CONTEXT: u32 = 64;
        let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../misaka-palw-tir-lower/tests/fixtures/hf/llama");
        if !fixture.join("model.safetensors").exists() {
            eprintln!("the llama fixture is missing: skipped");
            return;
        }
        let mut c = rcore::Chain::new(armed());
        c.room = true;
        let bundle = rcore::bundle(&c.p);
        let config = std::fs::read_to_string(fixture.join("config.json")).unwrap();

        // 1. check-architecture (IR mode): lowerable and admitted by tir_admit_v1 on this network.
        let report = misaka_palw_sdk::check_architecture::check_ir_config_v1(&c.p, &config, false);
        assert!(report.verdict.is_admissible(), "{}", report.verdict);

        // 2. The converter: lowered at the class's window, calibrated, materialised, written.
        let opts = LowerOpts { max_window: Some(CONTEXT), ..Default::default() };
        let prep = fidelity::prepare(&config, &opts).expect("lowered");
        let ck = Checkpoint::open(&fixture).unwrap();
        let (params, _) = ParamStore::from_source(&prep.hl, &prep.binding, &ck).unwrap();
        let loader = Resident(std::sync::Arc::new(params));
        let calib = fidelity::random_sequences(prep.hl.vocab, 4, 32, 7);
        let quiet = |_: usize, _: usize| {};
        let stats = fidelity::calibrate(&prep.hl, &loader, &calib, &quiet).unwrap();
        let mat = materialise(&prep.lowered, &prep.hl, &loader, &stats, &QuantPolicy::default(), &quiet).unwrap();
        let dir = std::env::temp_dir().join(format!("kaspad-tir-hf-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let lowered = dir.join("llama.palwtir");
        artifact::write(&lowered, &prep.lowered.program, &mat.params, [3u8; 64], serde_json::json!({})).unwrap();

        // 3. declare-layout: the class admission v10 admits (at this chain's fence).
        let declared = dir.join("llama.class.palwtir");
        let choice = misaka_palw_sdk::tir_layout::TirLayoutChoiceV1 { max_context: Some(CONTEXT), ..Default::default() };
        let d =
            misaka_palw_sdk::tir_layout::tir_declare_layout_v1(&c.p, &bundle, &lowered, &declared, &choice, Some("test/llama-tiny"))
                .expect("declared");
        assert_eq!(d.admission, Ok(()), "admission v10 {}", d.admission_at);
        assert_eq!(d.layout.max_context, CONTEXT);

        // 4. The node's holding: the IR lineage loads it; its backend runs the job a seat checks.
        let entry = misaka_palw_sdk::lineages::tir::TirLineageV1::open_entry(&declared).expect("an IR class");
        assert_eq!((entry.class_id(), entry.model_id.as_str()), (d.class_id, "test/llama-tiny"));
        let form = c.p.palw_prompt_ids_form_v1();
        let backend = misaka_palw_sdk::lineages::tir::TirLineageV1::backend(&entry, &bundle.court, form).expect("its backend");
        let (job, prompt) = PalwExecutionBackendV1::job_for_anchor(&backend, rcore::h(0xF00D)).expect("a job");
        let run = PalwExecutionBackendV1::execute(&backend, &job, &prompt).expect("the canonical job runs");
        let roots = PalwClaimRootsV1 {
            execution_root: run.execution_root,
            trace_root: run.trace_root,
            anchor: rcore::h(0xF00D),
            attempt_draw: None,
            output_root: None,
            job_pin: None,
        };
        assert_eq!(PalwExecutionBackendV1::verify_material(&backend, &run.material, roots), PalwMaterialVerdictV1::Matches);

        // 5. The registration, built and gated by the SDK as `--palw-register-class` builds it, folds.
        let (registrant, _, _) = rcore::floor_producer(&c.p);
        let (floor, _, target, slash) = rcore::genesis_classes(&c.p)[0];
        let terms = PalwRegistrationTermsV2 {
            min_grantable_share_permille: 0,
            slash_value_per_pwu: slash,
            initial_target: c.s.class_target(&floor).map(|t| t.target).unwrap_or(target),
            registered_class_ids: rcore::genesis_classes(&c.p).iter().map(|g| g.0).collect(),
            registered_artifact_roots: Vec::new(),
            chain_certified_families: Vec::new(),
        };
        let object = misaka_palw_sdk::tir_registration::build_tir_registration_v1(
            &c.p,
            &bundle,
            &entry,
            &terms,
            AT,
            registrant,
            vec![9; 16],
            AT,
        )
        .expect("admission v10 admits the lowered class");
        step(&mut c, AT, &[object], PalwBlockWorkV3::None, Hash64::default(), 0);
        empty(&mut c, AT + 1);
        assert!(c.s.tir_class_v1(&d.class_id).is_some(), "the tir_classes row");
        assert_eq!(lifecycle(&c, &d.class_id), Some(PalwModelLifecycleV1::Candidate), "a registered HF model opens Candidate");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Re-prove every non-registrant genesis seat's readiness for the current span, unless their
    /// rows are already this span's.
    fn keep_ready(c: &mut rcore::Chain, ir: &Ir, registrant: PalwBondKeyV2) {
        let span = span_daa(c, c.daa);
        let now = c.daa / span;
        let fresh = rcore::honest(&c.p)
            .iter()
            .filter(|k| **k != registrant)
            .all(|k| c.s.seat_readiness(k, &ir.class_id).is_some_and(|row| row.proved_span + 4 >= now));
        if !fresh {
            prove_all(c, ir, registrant, now, (c.daa + 1).max(now * span));
        }
    }

    /// Step to `target`, keeping the seats ready on the way (one block every few spans).
    fn advance_to(c: &mut rcore::Chain, ir: &Ir, registrant: PalwBondKeyV2, target: u64) {
        let span = span_daa(c, c.daa);
        while c.daa + 1 < target {
            keep_ready(c, ir, registrant);
            let next = (c.daa + 2 * span).min(target - 1).max(c.daa + 1);
            empty(c, next);
        }
        if c.daa < target {
            empty(c, target);
        }
    }

    fn job_of(material: &[u8]) -> kaspa_consensus_core::palw_v2::PalwJobContextV2 {
        misaka_palw_sdk::lineages::tir::TirCaptureV1::decode(material).expect("an IR capture").binding.job_context
    }

    fn prompt_of(material: &[u8]) -> Vec<usize> {
        misaka_palw_sdk::lineages::tir::TirCaptureV1::decode(material)
            .expect("an IR capture")
            .prompt
            .iter()
            .map(|t| *t as usize)
            .collect()
    }
}
