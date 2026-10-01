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
/// The step ladder this harness's fold holds a step demand's unit to (`step_c` folds under default extras: no held ladder, so
/// the release's `PALW_STEP_LEG_MAX_LEAVES`, 2^22) — what the builder is given, as a node on a network with no held regime gives it.
const DEMAND_LADDER: u64 = kaspa_consensus_core::palw_step_leg::PALW_STEP_LEG_MAX_LEAVES;
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

/// **A class with no dissected point** — the golden `moe-top2-shared` program (a routed MLP, no
/// reduction over the history): what the bisection court's own tests run on, since F7 gives a class
/// with a dissected cone its IR history dissection as the terminal move (a root claim at a dissected
/// leaf, a declared close elsewhere), which the ladder below does not play.
fn ir_class_undissected(tag: &str, tiled: bool) -> Ir {
    ir_class_of("moe-top2-shared", tag, tiled, court(), FORM)
}

/// [`ir_class`] served under a network's own court and prompt form.
fn ir_class_with(tag: &str, tiled: bool, court: PalwCourtParamsV2, form: PalwPromptIdsFormV1) -> Ir {
    ir_class_of("dense-gqa-2layer", tag, tiled, court, form)
}

/// [`ir_class_with`] over the golden program vector `program`.
fn ir_class_of(program: &str, tag: &str, tiled: bool, court: PalwCourtParamsV2, form: PalwPromptIdsFormV1) -> Ir {
    use kaspa_consensus_core::palw_step_refute::{PALW_LOGITS_TILE_LANES, flat_logits_scheme_id_v1, tiled_logits_scheme_id_v1};
    use kaspa_consensus_core::palw_tir_class_v1::{PALW_TIR_LAYOUT_VERSION_V1, PalwTirLayoutV1};
    use misaka_palw_tir::TirProgramV1;
    let vector = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../consensus-vectors/tir-v1/programs/{program}.json"));
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

/// The base class and three bonds at 100 — and the IR class, registered by its own object
/// (`ClassRegisteredTirV1`, F6 C(1)): its `tir_classes` row holds the program every IR object the
/// chain adjudicates is filled from (decision 2).
fn registry_with(ir: &Ir) -> PalwChainStateV2 {
    let s = step(&PalwChainStateV2::genesis(), 100, &registry_objects(ir)).expect("the registry");
    assert!(s.tir_class_v1(&ir.class_id).is_some(), "the IR class's row, with its program");
    s
}

/// [`registry_with`]'s objects: the base class, three bonds, and the IR class by its own object.
fn registry_objects(ir: &Ir) -> Vec<PalwConsensusObjectV2> {
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
    let entry = misaka_palw_sdk::tir_registration::tir_entries_of_v1(ir.registry.holdings()).remove(0);
    let ir_class = PalwConsensusObjectV2::ClassRegisteredTirV1 {
        class_id: ir.class_id,
        artifact_root: ir.root,
        slash_value_per_pwu: 5,
        pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: 40 },
        initial_target: u128::MAX / 2,
        share_permille: 100,
        activation_daa: 0,
        admission: Box::new(kaspa_consensus_core::palw_tir_class_v1::PalwTirAdmissionCarriageV1 {
            class: entry.class.as_ref().clone(),
            canonical: entry.canonical_context(),
            registrant_bond: bond_key(PRODUCER),
            signature: vec![9; 8],
        }),
    };
    vec![class(h64(1), h64(11), 1000), bond(PRODUCER), bond(SEAT), bond(COLLUDER), ir_class]
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
    produce_with(ir, n, lie, false)
}

/// [`produce`] — an honest run's capture a FOLD when `fold` (`with_dense_capture_bytes(0)`: a large
/// class's shape, whose leaves the executor re-derives to answer for them).
fn produce_with(ir: &Ir, n: u64, lie: Option<u64>, fold: bool) -> Claim {
    let backend: Box<dyn PalwExecutionBackendV1> = if fold { Box::new(ir.tir().with_dense_capture_bytes(0)) } else { ir.backend() };
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
        // Narrowed: the terminal move is the close — or, at a dissected leaf of an IR class, the
        // responder's root claim (RFC-0002 F7), which the fold clocks as `AwaitDisclosure`.
        if r.terminal_index.is_some() {
            return (s, daa);
        }
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
        let ir = ir_class_undissected(if tiled { "honest-tiled" } else { "honest-flat" }, tiled);
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
        let ir = ir_class_undissected(if tiled { "lie-tiled" } else { "lie-flat" }, tiled);
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
            let (label, mut accusation) = palw_tir_one_move_accusation_to_file_v1(
                case.candidates,
                &target,
                &tir.class().program,
                bond_key(SEAT),
                &court(),
                LADDER,
                FORM,
            )
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
        let as_filed = |built: Result<PalwCourtVerdictProofV2, String>| {
            built.map(|mut proof| {
                proof.tir_strip_program_v1();
                proof
            })
        };
        let against = vec![
            ("cone", as_filed(tir.cone_close(&honest.material, 3, &rules))),
            ("decode token", as_filed(tir.decode_token_close(&honest.material, 0, wrong_beat))),
        ];
        assert!(
            palw_tir_one_move_accusation_to_file_v1(against, &target, &tir.class().program, bond_key(SEAT), &court(), LADDER, FORM)
                .is_none(),
            "tiled {tiled}: nothing convicts an honest claim"
        );
    }
}

/// **A lie at a DISSECTED leaf is challenged by name** (RFC-0002 F7, the seat's side): on a class
/// whose history cones are dissected, the seat's one-move case parts from the lie at the dissected leaf
/// and its cone candidate becomes the named-leaf proof — the accused's leaf and its opening, nothing
/// else — which the chain's one-move gate reads as the challenge that opens a dissection there
/// (`NeedsDissection` under the held regime), declared `ExecutorGuilty`. A lie at an undissected commit
/// leaf of the same class is accused at its cone and convicted in one move, as before.
#[test]
fn a_lie_at_a_dissected_leaf_is_challenged_by_name() {
    use super::tir_court::{
        PALW_TIR_NAMED_LEAF_LABEL_V1, palw_tir_leaf_is_dissected_v1, palw_tir_one_move_accusation_to_file_v1,
        palw_tir_one_move_case_at_dissected_leaf_v1, palw_tir_one_move_case_v1,
    };
    use kaspa_consensus_core::palw_producer_v2::palw_disputable_claims_v2;
    use kaspa_consensus_core::palw_tir_one_move_v1::{PalwTirOneMoveOutcomeV1, palw_tir_one_move_outcome_v1};
    use kaspa_consensus_core::palw_tir_step_v1::PalwTirLeafKindV1;
    let ir = ir_class("named-leaf", true);
    let tir = ir.tir();
    let rules = tir.court_rules(&court());
    let s = registry_with(&ir);
    let honest = produce(&ir, 6, None);
    let ctx = misaka_palw_sdk::lineages::tir::TirCaptureV1::decode(&honest.material).unwrap().binding.job_context;
    let n =
        kaspa_consensus_core::palw_tir_step_v1::PalwTirStepSpaceV1::new(tir.class()).unwrap().leaf_count_capped(&ctx, LADDER).unwrap();
    let dissected = (0..n).find(|&i| palw_tir_leaf_is_dissected_v1(&tir, &ctx, i)).expect("the class dissects its history cones");
    let undissected = (dissected + 1..n)
        .find(|&i| {
            !palw_tir_leaf_is_dissected_v1(&tir, &ctx, i)
                && tir.space().leaf_at(&ctx, i).is_some_and(|l| matches!(l.kind, PalwTirLeafKindV1::Commit { .. }))
        })
        .expect("an undissected commit leaf after it");
    for (lie, at_dissected) in [(dissected, true), (undissected, false)] {
        let liar = produce(&ir, 6, Some(lie));
        assert_ne!(liar.env.attempt.execution_root, honest.env.attempt.execution_root, "leaf {lie}: the lie is committed");
        let s = licensed(&s, &liar, 101);
        let case = palw_tir_one_move_case_v1(&tir, &liar.material, &honest.material, &rules).unwrap().expect("a case");
        assert_eq!(case.leaf, Some(lie), "the case names the planted leaf");
        let (case, named) = palw_tir_one_move_case_at_dissected_leaf_v1(&tir, &liar.material, case, &rules);
        assert_eq!(named, at_dissected.then_some(lie), "leaf {lie}");
        assert_eq!(case.candidates.iter().any(|(label, _)| *label == "cone"), !at_dissected, "leaf {lie}");
        let target = palw_disputable_claims_v2(&s, &[bond_key(SEAT)]).into_iter().find(|t| t.claim_id == liar.id).expect("disputable");
        let (label, mut accusation) = palw_tir_one_move_accusation_to_file_v1(
            case.candidates,
            &target,
            &tir.class().program,
            bond_key(SEAT),
            &court(),
            LADDER,
            FORM,
        )
        .unwrap_or_else(|| panic!("leaf {lie}: an accusation is filed"));
        assert_eq!(accusation.verdict, PalwCourtVerdictV2::ExecutorGuilty);
        accusation.signature = vec![3; 16];
        let claim = s.claim(&liar.id).expect("the claim");
        let outcome = palw_tir_one_move_outcome_v1(&s, claim, &accusation, &court(), LADDER, FORM, true).expect("the gate reads it");
        if at_dissected {
            assert_eq!(label, PALW_TIR_NAMED_LEAF_LABEL_V1);
            assert!(accusation.proof.tir_binding_v1().is_some_and(|b| b.class.program.is_empty()), "it rides without its program");
            assert_eq!(outcome, PalwTirOneMoveOutcomeV1::NeedsDissection { leaf: lie }, "the named leaf opens the dissection");
        } else {
            assert_eq!(label, "cone", "a lie at an undissected leaf is accused at its cone");
            assert_eq!(outcome, PalwTirOneMoveOutcomeV1::Verdict(PalwCourtVerdictV2::ExecutorGuilty));
        }
    }
}

/// **RFC-0002 F7's node side, played against the fold** — every move of an IR history dissection built
/// by the node's own builders (`palw_panel::tir_dissect`) from the party's own capture, read off the
/// chain's duty view, and folded: the bisection route narrows to a DISSECTED leaf (the held regime
/// reaches the same leaf by a named-leaf challenge).
///
/// * **An honest producer falsely accused there defends itself to the bottom**: its root claim passes
///   the acceptance layer's finalize, every round folds, the challenger (whose own execution parts from
///   it only in the dissected leaf's output) finds no child to name — every child is its own
///   computation too, the node files nothing and says why — and, the child named for it, the bottom
///   the responder builds from its own capture acquits it: `ChallengerDefeated`, the claim live.
/// * **A liar at the dissected leaf cannot open its defence**: its capture's committed leaf is not its
///   evaluation, so the root claim does not finalize and the node does not file it; its silence is
///   the fold's no-show at the rung, which voids the claim.
#[test]
fn an_ir_history_dissection_is_played_by_the_node() {
    use super::tir_court::palw_tir_leaf_is_dissected_v1;
    use super::tir_dissect::{
        PalwTirDissectBuiltV1, PalwTirDissectMoveV1, palw_tir_dissect_build_v1, palw_tir_dissect_move_of_duty_v1,
        palw_tir_dissect_object_v1,
    };
    use kaspa_consensus_core::palw_court_v2::{adjudicate_court_close_v3, check_court_tir_root_claim_admits_v1};
    use kaspa_consensus_core::palw_tir_dissect_v1::{PALW_TIR_DISSECT_OBJECT_VERSION_V1, PalwTirDissectChoiceV1};
    for tiled in [false, true] {
        let ir = ir_class(if tiled { "f7-tiled" } else { "f7-flat" }, tiled);
        let tir = ir.tir();
        let rules = tir.court_rules(&court());
        let arity = court().dissection_arity();
        let honest = produce(&ir, 7, None);
        let ctx = misaka_palw_sdk::lineages::tir::TirCaptureV1::decode(&honest.material).unwrap().binding.job_context;
        let n = kaspa_consensus_core::palw_tir_step_v1::PalwTirStepSpaceV1::new(tir.class())
            .unwrap()
            .leaf_count_capped(&ctx, LADDER)
            .unwrap();
        // The LAST dissected leaf of the job: the longest history, so the dissection has rounds to play.
        let leaf = (0..n).rev().find(|&i| palw_tir_leaf_is_dissected_v1(&tir, &ctx, i)).expect("a dissected leaf");
        let sign = |_: &[u8], _: &[u8]| Some(vec![5u8; 8]);

        // ---- an honest producer, falsely accused at the dissected leaf ----
        let wrong = produce(&ir, 7, Some(leaf));
        let s = licensed(&registry_with(&ir), &honest, 101);
        let (s, sid) = open_court(&s, &honest, SEAT, 104);
        let (mut s, mut daa) = play_ladder(&ir, s, sid, &honest.material, &wrong.material, SEAT, false, 105);
        assert_eq!(duty_of(&s, SEAT, sid).and_then(|d| d.terminal_index), Some(leaf), "tiled {tiled}: the ladder lands on it");
        let mut played = Vec::new();
        loop {
            let (r, c) = (duty_of(&s, PRODUCER, sid), duty_of(&s, SEAT, sid));
            let Some((duty, own)) = [(r, &honest.material), (c, &wrong.material)]
                .into_iter()
                .find_map(|(d, own)| d.filter(|d| palw_tir_dissect_move_of_duty_v1(d).is_some()).map(|d| (d, own)))
            else {
                panic!("tiled {tiled}: a live session with no party to move ({played:?})");
            };
            let mv = palw_tir_dissect_move_of_duty_v1(&duty).unwrap();
            played.push(mv);
            let built = palw_tir_dissect_build_v1(&tir, &duty, mv, own, Some(&honest.material), None, &rules);
            let object = match (mv, built) {
                (PalwTirDissectMoveV1::Choice, Err(why)) => {
                    // The challenger finds no child to name: its own execution agrees with every one.
                    assert!(why.contains("no child to name"), "tiled {tiled}: {why}");
                    let phase = duty.tir_dissection.as_deref().expect("a phase");
                    let choice = PalwTirDissectChoiceV1 {
                        version: PALW_TIR_DISSECT_OBJECT_VERSION_V1,
                        session_id: sid,
                        round: phase.round(),
                        child: 0,
                    };
                    palw_tir_dissect_object_v1(PalwTirDissectBuiltV1::Choice(choice), &duty, arity, &sign, &|_, _| None)
                        .unwrap()
                        .unwrap()
                }
                (mv, built) => {
                    let built = built.unwrap_or_else(|e| panic!("tiled {tiled}: the {mv:?} builds: {e}"));
                    if let PalwTirDissectBuiltV1::Root(root) = &built {
                        check_court_tir_root_claim_admits_v1(&s, &sid, root, arity, arity, &court(), LADDER, FORM)
                            .unwrap_or_else(|e| panic!("tiled {tiled}: the acceptance layer admits the root claim: {e}"));
                    }
                    let verdict_of = |sid: &Hash64, proof: &PalwCourtVerdictProofV2| {
                        adjudicate_court_close_v3(&s, sid, proof, &court(), LADDER, FORM, false, false).ok()
                    };
                    palw_tir_dissect_object_v1(built, &duty, arity, &sign, &verdict_of)
                        .unwrap_or_else(|e| panic!("tiled {tiled}: the {mv:?} is filed: {e}"))
                        .unwrap_or_else(|| panic!("tiled {tiled}: the {mv:?} wins this party's side"))
                }
            };
            let closing = matches!(object, PalwConsensusObjectV2::CourtClosed { .. });
            if let PalwConsensusObjectV2::CourtClosed { verdict, .. } = &object {
                assert_eq!(*verdict, PalwCourtVerdictV2::ChallengerDefeated, "tiled {tiled}: the bottom acquits the honest leaf");
                assert!(duty.i_am_responder, "tiled {tiled}: the acquittal is the responder's to file");
            }
            s = step(&s, daa, &[object]).unwrap_or_else(|e| panic!("tiled {tiled}: the {mv:?} folds: {e}"));
            daa += 1;
            if closing {
                break;
            }
            assert!(played.len() < 64, "tiled {tiled}: the dissection ends");
        }
        assert_eq!(played.first(), Some(&PalwTirDissectMoveV1::Root), "tiled {tiled}");
        assert!(played.contains(&PalwTirDissectMoveV1::Round) && played.contains(&PalwTirDissectMoveV1::Choice), "{played:?}");
        assert!(s.court_session(&sid).is_none() && s.tir_dissection_v1(&sid).is_none(), "tiled {tiled}: the session ended");
        assert!(!matches!(phase_of(&s, &honest.id), PalwClaimPhaseV2::Voided { .. }), "tiled {tiled}: the honest claim stands");

        // ---- a liar at the dissected leaf ----
        let liar = produce(&ir, 8, Some(leaf));
        let honest_twin = produce(&ir, 8, None);
        let s = licensed(&registry_with(&ir), &liar, 101);
        let (s, sid) = open_court(&s, &liar, SEAT, 104);
        let (s, _) = play_ladder(&ir, s, sid, &liar.material, &honest_twin.material, SEAT, false, 105);
        let duty = duty_of(&s, PRODUCER, sid).expect("the responder's duty");
        assert_eq!(palw_tir_dissect_move_of_duty_v1(&duty), Some(PalwTirDissectMoveV1::Root), "tiled {tiled}");
        let refused =
            palw_tir_dissect_build_v1(&tir, &duty, PalwTirDissectMoveV1::Root, &liar.material, Some(&liar.material), None, &rules)
                .expect_err("a lying leaf's root claim does not finalize");
        assert!(refused.contains("does not finalize to the committed tile"), "tiled {tiled}: {refused}");
        let s = step(&s, duty.rung_deadline_daa + 1, &[]).expect("the clock runs");
        assert!(matches!(phase_of(&s, &liar.id), PalwClaimPhaseV2::Voided { .. }), "tiled {tiled}: the liar's silence voids it");
    }
}

/// **A lie is accused from served annexes — no capture moves** (RFC-0002's evidence transport, option
/// B). The seat holds its own execution of the claim's job and nothing of the accused's; the executor
/// serves one annex per leaf asked (`TirBackendV1::leaf_annex`: binding, the leaf's preimage and
/// opening, the trace summary), each verified by hash arithmetic against the claim's roots
/// (`palw_tir_leaf_annex_verify_v1`: a carried program, another claim's roots or a doctored preimage is
/// refused). The descent (`tir_first_divergence_from_opening_v1`) names the first differing leaf in at
/// most ⌈log₂ n⌉ + 1 annexes, and the accusation is built over the seat's own execution and that one
/// annex:
///
/// * a lie at an undissected commit leaf: its cone close, which the one-move gate reads `ExecutorGuilty`;
/// * a lie at a dissected leaf: the named-leaf challenge, which the gate reads as the dissection it opens;
/// * a wrong token over honest arithmetic: the descent agrees at once, the ids say which row, and the
///   annex of the logits leaf whose tile holds the seat's own token carries the pin of the decode-token
///   door, which convicts;
/// * an honest claim: the first annex agrees, and nothing is built.
#[test]
fn a_lie_is_accused_from_served_annexes_without_the_capture() {
    use super::tir_court::{
        palw_tir_leaf_is_dissected_v1, palw_tir_one_move_names_a_dissected_leaf_v1, palw_tir_one_move_verdict_stateless_v1,
    };
    use kaspa_consensus_core::palw_producer_v2::palw_disputable_claims_v2;
    use kaspa_consensus_core::palw_tir_step_v1::PalwTirLeafKindV1;
    use misaka_palw_sdk::lineages::tir::{
        PalwTirLeafAnnexV1, TirDivergenceV1, TirStepTreeV1, palw_tir_leaf_annex_verify_v1, tir_first_divergence_from_opening_v1,
    };
    let ir = ir_class("annex", true);
    let tir = ir.tir();
    let rules = tir.court_rules(&court());
    let program = tir.class().program.clone();
    let s = registry_with(&ir);
    let honest = produce(&ir, 11, None);
    let ctx = misaka_palw_sdk::lineages::tir::TirCaptureV1::decode(&honest.material).unwrap().binding.job_context;
    let prompt: Vec<u32> = honest.prompt.iter().map(|t| *t as u32).collect();
    // The seat's own execution of the job, and its tree.
    let own = tir.retain_memo(&ctx, &prompt).expect("the seat's own run");
    let own_tree = TirStepTreeV1::full(&own.leaf_hashes);
    let n = own.leaf_hashes.len() as u64;
    let depth = 64 - (n - 1).leading_zeros() as usize;
    // The descent against a claim's served annexes: each asked, encoded, decoded and verified.
    let serve = |claim: &Claim, leaf: u64| -> (PalwTirLeafAnnexV1, kaspa_consensus_core::palw_tir_step_v1::PalwTirStepBindingV1) {
        let bytes = tir.leaf_annex(&claim.material, leaf).expect("the executor serves the annex").encode();
        let annex = PalwTirLeafAnnexV1::decode(&bytes).expect("an annex");
        let binding =
            palw_tir_leaf_annex_verify_v1(&annex, &program, claim.env.attempt.execution_root, claim.env.attempt.trace_root, LADDER)
                .expect("the annex is the claim's");
        (annex, binding)
    };
    let descend = |claim: &Claim| -> Option<(PalwTirLeafAnnexV1, kaspa_consensus_core::palw_tir_step_v1::PalwTirStepBindingV1)> {
        let (mut j, mut below) = (0u64, None);
        for _ in 0..=depth {
            let (annex, binding) = serve(claim, j);
            match tir_first_divergence_from_opening_v1(&own_tree, &annex.opening, below).expect("of the job's shape") {
                TirDivergenceV1::At(_) => return Some((annex, binding)),
                TirDivergenceV1::Within { level, first } => (j, below) = (first, Some(level)),
                TirDivergenceV1::Agrees => return None,
            }
        }
        panic!("the descent did not end in ⌈log₂ n⌉ + 1 annexes");
    };
    // An honest claim: the first annex agrees.
    assert!(descend(&honest).is_none(), "an honest claim's annexes agree with the seat's own run");
    // Tampering is refused by arithmetic.
    let (good, _) = serve(&honest, 0);
    let mut carried = good.clone();
    carried.binding.class.program = program.clone();
    assert!(
        palw_tir_leaf_annex_verify_v1(&carried, &program, honest.env.attempt.execution_root, honest.env.attempt.trace_root, LADDER)
            .is_err()
    );
    let mut doctored = good.clone();
    doctored.preimage.values_le[0] ^= 1;
    assert!(
        palw_tir_leaf_annex_verify_v1(&doctored, &program, honest.env.attempt.execution_root, honest.env.attempt.trace_root, LADDER)
            .is_err()
    );
    assert!(
        palw_tir_leaf_annex_verify_v1(&good, &program, h64(1), honest.env.attempt.trace_root, LADDER).is_err(),
        "another claim's roots"
    );

    let dissected = (0..n).find(|&i| palw_tir_leaf_is_dissected_v1(&tir, &ctx, i)).expect("a dissected leaf");
    let undissected = (dissected + 1..n)
        .find(|&i| {
            !palw_tir_leaf_is_dissected_v1(&tir, &ctx, i)
                && tir.space().leaf_at(&ctx, i).is_some_and(|l| matches!(l.kind, PalwTirLeafKindV1::Commit { .. }))
        })
        .expect("an undissected commit leaf");
    for (lie, at_dissected) in [(undissected, false), (dissected, true)] {
        let liar = produce(&ir, 11, Some(lie));
        let s = licensed(&s, &liar, 101);
        let target = palw_disputable_claims_v2(&s, &[bond_key(SEAT)]).into_iter().find(|t| t.claim_id == liar.id).expect("disputable");
        let (annex, binding) = descend(&liar).expect("the lie is found");
        assert_eq!(annex.leaf(), lie, "the descent names the planted leaf");
        if at_dissected {
            let proof = tir.annex_named_leaf(&binding, &annex, &own).expect("the named-leaf proof");
            let mut proof = proof;
            proof.tir_strip_program_v1();
            assert!(palw_tir_one_move_names_a_dissected_leaf_v1(&proof, &target, &program, &court()), "it opens the dissection");
        } else {
            let mut proof = tir.annex_cone_close(&binding, &annex, &own, &rules).expect("the cone close");
            proof.tir_strip_program_v1();
            assert_eq!(
                palw_tir_one_move_verdict_stateless_v1(&proof, &target, &program, &court(), LADDER, FORM),
                Some(PalwCourtVerdictV2::ExecutorGuilty),
                "the cone close convicts the lie"
            );
        }
    }

    // A wrong token over honest arithmetic: no step differs; the ids name the row, and the annex of the
    // logits leaf holding the seat's own token carries the decode-token door's pin.
    let token_liar = craft(&ir, &honest, |c| c.generated[0] = (c.generated[0] + 1) % c.logits_rows[0].len() as u32);
    let s2 = licensed(&s, &token_liar, 101);
    let target =
        palw_disputable_claims_v2(&s2, &[bond_key(SEAT)]).into_iter().find(|t| t.claim_id == token_liar.id).expect("disputable");
    assert!(descend(&token_liar).is_none(), "every step leaf is the honest execution's");
    let (first, _) = serve(&token_liar, 0);
    let row = first.generated().iter().zip(&own.generated).position(|(a, b)| a != b).expect("the ids differ") as u32;
    let mine = own.generated[row as usize];
    let position = ctx.declared_prefill_tokens + row - 1;
    let space = tir.space();
    let post = (space.occurrences().len() - 1) as u32;
    let leaf = space
        .leaves_of_position(&ctx, position)
        .into_iter()
        .find(|l| {
            matches!(l.kind, PalwTirLeafKindV1::Commit { occurrence, node, first_element, .. }
                if occurrence == post && node == space.program.logits
                    && (first_element..first_element + u64::from(l.value_count)).contains(&u64::from(mine)))
        })
        .expect("the logits leaf holding the seat's token")
        .index;
    let (annex, binding) = serve(&token_liar, leaf);
    let mut proof = tir.annex_decode_token_close(&binding, &annex, row, mine).expect("the decode-token door");
    proof.tir_strip_program_v1();
    assert_eq!(
        palw_tir_one_move_verdict_stateless_v1(&proof, &target, &program, &court(), LADDER, FORM),
        Some(PalwCourtVerdictV2::ExecutorGuilty),
        "the wrong token is convicted from the annex"
    );
}

/// **The seat's annex pursuit, step by step** (option B's node half, `palw_tir_annex_step_v1`): from
/// the first annex asked (leaf 0) the pursuit's own steps — each annex served by the executor's backend,
/// under the request index the lane keys it by — reach the accusation the capture path would file: the
/// cone close at an undissected lie (the gate convicts), the named leaf at a dissected one (it opens the
/// dissection), and for a wrong token the decode-token door on the pin of the logits leaf holding the
/// seat's own token. An honest claim's first annex ends the pursuit with nothing to file; an annex of
/// another leaf than the one asked is not stepped on. And the panel wires it: the pass pursues a claim
/// whose capture it does not hold, asks through the lane's leaf request, and the executor serves the
/// annex of its own IR capture.
#[test]
fn a_seat_pursues_a_large_claim_through_served_annexes() {
    use super::tir_court::{
        PalwTirAnnexPursuitV1, PalwTirAnnexStepV1, palw_tir_annex_request_index_v1, palw_tir_annex_step_v1,
        palw_tir_leaf_is_dissected_v1, palw_tir_one_move_accusation_to_file_v1,
    };
    use kaspa_consensus_core::palw_producer_v2::palw_disputable_claims_v2;
    use kaspa_consensus_core::palw_tir_one_move_v1::{PalwTirOneMoveOutcomeV1, palw_tir_one_move_outcome_v1};
    use kaspa_consensus_core::palw_tir_step_v1::PalwTirLeafKindV1;
    use misaka_palw_sdk::lineages::tir::{PalwTirLeafAnnexV1, palw_tir_leaf_annex_verify_v1};
    let ir = ir_class("pursuit", true);
    let tir = ir.tir();
    let rules = tir.court_rules(&court());
    let program = tir.class().program.clone();
    let s = registry_with(&ir);
    let honest = produce(&ir, 13, None);
    let ctx = misaka_palw_sdk::lineages::tir::TirCaptureV1::decode(&honest.material).unwrap().binding.job_context;
    let prompt: Vec<u32> = honest.prompt.iter().map(|t| *t as u32).collect();
    let own = tir.retain_memo(&ctx, &prompt).expect("the seat's own run");
    let n = own.leaf_hashes.len() as u64;
    // The request index keys each leaf's answer on the lane: distinct leaves, distinct slots.
    let idx: std::collections::HashSet<u32> = (0..n).map(palw_tir_annex_request_index_v1).collect();
    assert_eq!(idx.len() as u64, n, "one lane slot per leaf of the job");
    assert!(idx.iter().all(|i| kaspa_consensus_core::palw_leaf_evidence_v1::palw_leaf_evidence_request_decode_v1(*i).is_some()));
    // Drive the pursuit through the executor's served annexes.
    let pursue = |claim: &Claim| -> (u32, PalwTirAnnexStepV1) {
        let mut pursuit = PalwTirAnnexPursuitV1::new(own.clone(), 100);
        for round in 0..64u32 {
            let bytes = tir.leaf_annex(&claim.material, pursuit.leaf).expect("served").encode();
            let annex = PalwTirLeafAnnexV1::decode(&bytes).unwrap();
            let binding = palw_tir_leaf_annex_verify_v1(
                &annex,
                &program,
                claim.env.attempt.execution_root,
                claim.env.attempt.trace_root,
                LADDER,
            )
            .expect("verified");
            match palw_tir_annex_step_v1(&tir, &pursuit, &annex, &binding, &rules) {
                PalwTirAnnexStepV1::Ask { leaf, below, token } => (pursuit.leaf, pursuit.below, pursuit.token) = (leaf, below, token),
                end => return (round + 1, end),
            }
        }
        panic!("the pursuit does not end")
    };
    // An honest claim: the first annex ends it, nothing to file.
    assert!(matches!(pursue(&honest).1, PalwTirAnnexStepV1::Stop(_)));
    // An annex of another leaf is not stepped on.
    {
        let pursuit = PalwTirAnnexPursuitV1::new(own.clone(), 100);
        let other = PalwTirLeafAnnexV1::decode(&tir.leaf_annex(&honest.material, 1).unwrap().encode()).unwrap();
        let b =
            palw_tir_leaf_annex_verify_v1(&other, &program, honest.env.attempt.execution_root, honest.env.attempt.trace_root, LADDER)
                .unwrap();
        assert!(matches!(palw_tir_annex_step_v1(&tir, &pursuit, &other, &b, &rules), PalwTirAnnexStepV1::Stop(_)));
    }
    let depth = 64 - (n - 1).leading_zeros();
    let dissected = (0..n).find(|&i| palw_tir_leaf_is_dissected_v1(&tir, &ctx, i)).expect("a dissected leaf");
    let undissected = (dissected + 1..n)
        .find(|&i| {
            !palw_tir_leaf_is_dissected_v1(&tir, &ctx, i)
                && tir.space().leaf_at(&ctx, i).is_some_and(|l| matches!(l.kind, PalwTirLeafKindV1::Commit { .. }))
        })
        .expect("an undissected commit leaf");
    for (lie, at_dissected) in [(undissected, false), (dissected, true)] {
        let liar = produce(&ir, 13, Some(lie));
        let s = licensed(&s, &liar, 101);
        let target = palw_disputable_claims_v2(&s, &[bond_key(SEAT)]).into_iter().find(|t| t.claim_id == liar.id).expect("disputable");
        let (rounds, end) = pursue(&liar);
        assert!(rounds <= depth + 1, "{rounds} annexes for {n} leaves");
        let PalwTirAnnexStepV1::Accuse { leaf, candidates, .. } = end else { panic!("an accusation") };
        assert_eq!(leaf, Some(lie), "the pursuit names the planted leaf");
        let (label, mut accusation) =
            palw_tir_one_move_accusation_to_file_v1(candidates, &target, &program, bond_key(SEAT), &court(), LADDER, FORM)
                .expect("an accusation to file");
        accusation.signature = vec![3; 16];
        let claim = s.claim(&liar.id).expect("the claim");
        let outcome = palw_tir_one_move_outcome_v1(&s, claim, &accusation, &court(), LADDER, FORM, true).expect("the gate reads it");
        if at_dissected {
            assert_eq!(label, super::tir_court::PALW_TIR_NAMED_LEAF_LABEL_V1);
            assert_eq!(outcome, PalwTirOneMoveOutcomeV1::NeedsDissection { leaf: lie });
        } else {
            assert_eq!(label, "cone");
            assert_eq!(outcome, PalwTirOneMoveOutcomeV1::Verdict(PalwCourtVerdictV2::ExecutorGuilty));
        }
    }
    // A wrong token: the pursuit agrees on every step, asks the logits leaf holding its own token, and
    // files the decode-token door.
    let token_liar = craft(&ir, &honest, |c| c.generated[0] = (c.generated[0] + 1) % c.logits_rows[0].len() as u32);
    let s2 = licensed(&s, &token_liar, 101);
    let target =
        palw_disputable_claims_v2(&s2, &[bond_key(SEAT)]).into_iter().find(|t| t.claim_id == token_liar.id).expect("disputable");
    let (rounds, end) = pursue(&token_liar);
    assert_eq!(rounds, 2, "the first annex agrees; the second is the logits leaf's");
    let PalwTirAnnexStepV1::Accuse { row, candidates, .. } = end else { panic!("an accusation") };
    assert_eq!(row, Some(0));
    let (label, mut accusation) =
        palw_tir_one_move_accusation_to_file_v1(candidates, &target, &program, bond_key(SEAT), &court(), LADDER, FORM).expect("filed");
    assert_eq!(label, "decode token");
    accusation.signature = vec![3; 16];
    let claim = s2.claim(&token_liar.id).expect("the claim");
    assert_eq!(
        palw_tir_one_move_outcome_v1(&s2, claim, &accusation, &court(), LADDER, FORM, true).expect("read"),
        PalwTirOneMoveOutcomeV1::Verdict(PalwCourtVerdictV2::ExecutorGuilty)
    );

    // The wiring: the pass pursues a claim whose capture it does not hold, through the lane's leaf
    // request; the executor serves the annex of its own IR capture on that request.
    let court_src = include_str!("tir_court.rs");
    let court_src = &court_src[..court_src.find("\n#[cfg(test)]").unwrap_or(court_src.len())];
    let pass = &court_src[court_src.find("    pub(super) async fn tir_one_move_pass_v1(").expect("the pass")..];
    assert!(pass.contains(
        "self.tir_annex_pursuit_tick_v1(session, bond_key, network_domain, current_daa, &target, due, tir, &mut books).await;"
    ));
    let tick = &court_src[court_src.find("    async fn tir_annex_pursuit_tick_v1(").expect("the tick")..];
    // The first ask: leaf 0 — or, on a trace descent, decode row 0's door leaf (`with_trace`).
    let first_ask = &tick[tick.find("let mut pursuit = PalwTirAnnexPursuitV1::new(own, current_daa);").expect("the pursuit")..];
    let first_ask = &first_ask[..first_ask.find("            return;").expect("the first ask's end")];
    assert!(first_ask.contains("pursuit = pursuit.with_trace(rows, row0);"));
    assert!(first_ask.contains(
        "self.request_leaf_evidence_v1(network_domain, claim, palw_tir_annex_request_index_v1(leaf), leaf, current_daa).await;"
    ));
    assert!(tick.contains("palw_tir_leaf_annex_verify_v1("));
    assert!(tick.contains("palw_tir_annex_step_v1(&tir, &task, &annex, &binding, &rules)"));
    let panel = include_str!("../palw_panel.rs");
    let open = &panel[panel.find("    fn open_retained_interval(").expect("the opener")..];
    let serve =
        open.find("return self.serve_tir_leaf_annex_v1(claim, &capture, leaf);").expect("an IR claim's leaf is served as its annex");
    assert!(
        serve < open.find("self.retained_leaf_evidence_v1(").expect("the legacy evidence"),
        "the IR annex before the legacy evidence"
    );
    assert!(panel.contains("pursuits: &mut tir_annex_pursuits,") && panel.contains("openings: &interval_openings,"));
}

/// **F7's bottom from the accused's ON-CHAIN root claim** (RFC-0002's evidence transport, option D): a
/// challenger that holds no accused capture builds the bottom close from its own execution and the leaf
/// the accused's root claim carries (`TirBackendV1::dissect_bottom_from_root_claim`) — byte for byte the
/// canonical bottom the accused's own capture builds, so it adjudicates exactly as that one does.
#[test]
fn a_challenger_builds_the_dissection_bottom_from_the_on_chain_root_claim() {
    use super::tir_court::palw_tir_leaf_is_dissected_v1;
    use super::tir_dissect::{
        PalwTirDissectBuiltV1, PalwTirDissectMoveV1, palw_tir_dissect_build_v1, palw_tir_dissect_move_of_duty_v1,
        palw_tir_dissect_object_v1,
    };
    use kaspa_consensus_core::palw_court_v2::adjudicate_court_close_v3;
    let ir = ir_class("bottom-from-chain", true);
    let tir = ir.tir();
    let rules = tir.court_rules(&court());
    let arity = court().dissection_arity();
    let honest = produce(&ir, 12, None);
    let ctx = misaka_palw_sdk::lineages::tir::TirCaptureV1::decode(&honest.material).unwrap().binding.job_context;
    let n =
        kaspa_consensus_core::palw_tir_step_v1::PalwTirStepSpaceV1::new(tir.class()).unwrap().leaf_count_capped(&ctx, LADDER).unwrap();
    let leaf = (0..n).rev().find(|&i| palw_tir_leaf_is_dissected_v1(&tir, &ctx, i)).expect("a dissected leaf");
    let wrong = produce(&ir, 12, Some(leaf));
    let s = licensed(&registry_with(&ir), &honest, 101);
    let (s, sid) = open_court(&s, &honest, SEAT, 104);
    let (mut s, mut daa) = play_ladder(&ir, s, sid, &honest.material, &wrong.material, SEAT, false, 105);
    let sign = |_: &[u8], _: &[u8]| Some(vec![5u8; 8]);
    // Play to the bottom: the responder's root claim and rounds, the challenger's choices.
    let mut root = None;
    loop {
        let (r, c) = (duty_of(&s, PRODUCER, sid).unwrap(), duty_of(&s, SEAT, sid).unwrap());
        if r.tir_dissection.as_ref().is_some_and(|p| p.turn() == PalwBisectTurnV1::Terminal) {
            break;
        }
        let (duty, own) = if palw_tir_dissect_move_of_duty_v1(&r).is_some() { (r, &honest.material) } else { (c, &wrong.material) };
        let mv = palw_tir_dissect_move_of_duty_v1(&duty).expect("a move");
        let object = match palw_tir_dissect_build_v1(&tir, &duty, mv, own, None, None, &rules) {
            Ok(built) => {
                if let PalwTirDissectBuiltV1::Root(filed) = &built {
                    root = Some(filed.as_ref().clone());
                }
                palw_tir_dissect_object_v1(built, &duty, arity, &sign, &|_, _| None).unwrap().unwrap()
            }
            Err(why) if mv == PalwTirDissectMoveV1::Choice => {
                assert!(why.contains("no child to name"), "{why}");
                let phase = duty.tir_dissection.as_deref().unwrap();
                let choice = kaspa_consensus_core::palw_tir_dissect_v1::PalwTirDissectChoiceV1 {
                    version: kaspa_consensus_core::palw_tir_dissect_v1::PALW_TIR_DISSECT_OBJECT_VERSION_V1,
                    session_id: sid,
                    round: phase.round(),
                    child: 0,
                };
                palw_tir_dissect_object_v1(PalwTirDissectBuiltV1::Choice(choice), &duty, arity, &sign, &|_, _| None).unwrap().unwrap()
            }
            Err(why) => panic!("{mv:?}: {why}"),
        };
        s = step(&s, daa, &[object]).expect("the move folds");
        daa += 1;
    }
    let root = root.expect("the responder's root claim, as it rode");
    let c = duty_of(&s, SEAT, sid).unwrap();
    let phase = c.tir_dissection.as_deref().expect("the phase");
    // The challenger holds no accused capture: its bottom comes from the root claim on chain.
    let built = palw_tir_dissect_build_v1(&tir, &c, PalwTirDissectMoveV1::Close, &wrong.material, None, Some(&root), &rules)
        .expect("the bottom from the root claim");
    let PalwTirDissectBuiltV1::Close(from_chain) = built else { panic!("a close") };
    // The accused's own capture builds the canonical bottom; the two are one object.
    let canonical =
        palw_tir_dissect_build_v1(&tir, &c, PalwTirDissectMoveV1::Close, &wrong.material, Some(&honest.material), None, &rules)
            .expect("the bottom from the capture");
    let PalwTirDissectBuiltV1::Close(canonical) = canonical else { panic!("a close") };
    assert_eq!(from_chain, canonical, "the bottom from the chain is the canonical bottom");
    let verdict = adjudicate_court_close_v3(&s, &sid, &from_chain, &court(), LADDER, FORM, false, false).expect("it adjudicates");
    assert_eq!(verdict, PalwCourtVerdictV2::ChallengerDefeated, "an honest leaf's bottom acquits, whoever builds it");
    let _ = phase;
}

/// **A node started with `--palw-verify-class-manifest` holds an IR artifact by its IR manifest**
/// (Phase H: every testnet-12 seat runs the flag for its 8k artifact). The verification read every
/// sidecar as the legacy manifest, so an IR artifact failed it either way — with no sidecar ("have no
/// `.palwmanifest` beside them") and with the one `palw-class manifest` writes for a `PALWTIR1` file
/// (another schema) — and the node refused to start. The IR sidecar is now checked as what it is: the
/// inventory root re-derived from the file and compared; absent, or describing another file, is still
/// the refusal.
#[test]
fn an_ir_artifact_passes_the_startup_manifest_verification_by_its_ir_manifest() {
    use misaka_palw_sdk::tir_manifest::PalwTirManifestV1;
    let ir = ir_class("manifest", true);
    let holdings = ir.registry.holdings();
    let path = holdings[0].path.clone().expect("a file");
    let sidecar = misaka_palw_sdk::PalwClassManifestFileV1::path_beside(&path);
    let verify = || crate::palw_backends::verify_class_manifests_v1(ir.registry.sdk(), holdings);
    let absent = verify().expect_err("no sidecar: nothing verified");
    assert!(absent.contains("no `.palwmanifest`"), "{absent}");
    let m = PalwTirManifestV1::derive(&path).expect("the IR manifest");
    std::fs::write(&sidecar, m.to_json()).unwrap();
    assert_eq!(verify(), Ok(1), "the IR manifest re-derived from the file: one class verified");
    // A sidecar that describes another file is refused (the digest), and so is a root that is not the file's.
    let mut other = m.clone();
    other.artifact_digest[0] ^= 1;
    std::fs::write(&sidecar, other.to_json()).unwrap();
    assert!(verify().expect_err("another file's manifest").contains("different file"));
    let mut wrong_root = m.clone();
    wrong_root.inventory_root = Hash64::from_u64_word(7);
    std::fs::write(&sidecar, wrong_root.to_json()).unwrap();
    assert!(verify().expect_err("a wrong root").contains("inventory root"));
    std::fs::write(&sidecar, "{}").unwrap();
    assert!(verify().is_err(), "a sidecar that does not parse");
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
        let (record, _) = kaspa_consensus_core::palw_tir_admission_v1::palw_tir_class_record_v1(ir.tir().class(), &ir.root)
            .expect("the class's record");
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
            super::palw_da_claim_answers_v1(backend.as_ref(), &facts, vec![claim.material.clone()], |_| {}, &units, rows, false, None)
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
            // It rides without the program (decision 2): the chain fills the registered one back.
            let filled = disclosure.with_program_v1(&record).expect("an answer carries no program");
            check_tir_trace_event_disclosure_v1(
                claim.env.attempt.trace_root,
                claim.env.attempt.execution_root,
                row,
                tile,
                &filled,
                LADDER,
            )
            .unwrap_or_else(|e| panic!("tiled {tiled}: event ({row}, {tile}) is answered: {e}"));
            checked += 1;
        }
        assert!(checked >= 2 && out_of_range >= 1, "tiled {tiled}: {checked} checked, {out_of_range} out of range");
        // A held unit is not an IR claim's to answer.
        let held = [PalwDaUnitV1::Held(PalwHeldMissingV1::StepLeaf { leaf: 0 })];
        let answered =
            super::palw_da_claim_answers_v1(backend.as_ref(), &facts, vec![claim.material.clone()], |_| {}, &held, rows, false, None)
                .expect("the kept capture");
        assert!(matches!(&answered.answers[0], Some(Err(why)) if why.contains("held unit")), "{:?}", answered.answers[0]);
    }
}

// ---------------------------------------------------------------------------------------------
// RFC-0002 evidence transport C: the second IR fence's DA unit, through the node's doors
// ---------------------------------------------------------------------------------------------

/// The second IR fence's height in the option-C tests: past the registry (100), the claim (101) and
/// its panel (102).
const C_FENCE2: u64 = 103;

/// [`params`] with R-core+ in force from genesis (as on testnet-12: a demand opens a session in
/// `da_sessions`), the work ceiling at 500‰ (the other half is an accuser's room, A-6), and
/// `palw_tir_fence2` at [`C_FENCE2`].
fn params_c() -> PalwStateParamsV2 {
    params()
        .with_fp_exposure_ceiling(500)
        .expect("a ceiling")
        .with_rcore_plus_mirrors(Some(0), 0, Vec::new())
        .with_tir_fence2_from_daa(Some(C_FENCE2))
}

fn step_c(
    parent: &PalwChainStateV2,
    daa: u64,
    objects: &[PalwConsensusObjectV2],
    att: Option<&PalwAttemptEnvelopeV2>,
) -> Result<PalwChainStateV2, PalwStateV2Error> {
    let p = params_c();
    apply_palw_transition_v2_with_extras(
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
    )
    .map(|(child, _)| child)
}

/// The registry, the claim at 101 and its panel (SEAT, COLLUDER) at 102, on [`params_c`]: a live claim
/// — R-core+ demands a unit of a claim at any live stage.
fn bound_c(ir: &Ir, claim: &Claim) -> PalwChainStateV2 {
    let s = step_c(&PalwChainStateV2::genesis(), 100, &registry_objects(ir), None).expect("the registry");
    let s = step_c(&s, 101, &[], Some(&claim.env)).expect("the claim");
    let seats = [SEAT, COLLUDER]
        .map(|seat| PalwPanelSeatV2 { bond: bond_key(seat), operator_id: palw_operator_id_v2(&op_key(20 + seat)) })
        .to_vec();
    step_c(&s, 102, &[PalwConsensusObjectV2::PanelBound { claim: claim.id, anchor: h64(77), seats }], None).expect("the panel")
}

/// What the duty loop answers a claim's units under ([`super::PalwDaClaimFactsV1`]).
fn da_facts(ir: &Ir, claim: &Claim) -> super::PalwDaClaimFactsV1 {
    super::PalwDaClaimFactsV1 {
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
    }
}

/// The units `accuser`'s open session on `claim` demands, and its deadline.
fn session_c(
    s: &PalwChainStateV2,
    claim: &Hash64,
    accuser: u64,
) -> Option<(Vec<kaspa_consensus_core::palw_da_rcore_v1::PalwDaUnitV1>, u64)> {
    s.da_sessions_of(claim)
        .find(|(bond, _)| **bond == bond_key(accuser))
        .map(|(_, session)| (session.units.clone(), session.deadline_daa))
}

/// **The executor's answers to every unit of a session, built by the duty loop's own path**
/// (`palw_da_claim_answers_v1` over the kept capture, with the class's IR responder) and made the
/// objects that ride (`palw_da_answer_object_v1`, under the court's close ceiling).
fn node_answers_c(
    ir: &Ir,
    claim: &Claim,
    units: &[kaspa_consensus_core::palw_da_rcore_v1::PalwDaUnitV1],
) -> Vec<PalwConsensusObjectV2> {
    let backend = ir.backend();
    let tir = ir.tir();
    let built = super::palw_da_claim_answers_v1(
        backend.as_ref(),
        &da_facts(ir, claim),
        vec![claim.material.clone()],
        |_| {},
        units,
        claim.job.exact_decode_tokens,
        false,
        Some(&tir),
    )
    .expect("the kept capture answers");
    units
        .iter()
        .zip(built.answers)
        .map(|(unit, answer)| {
            let answer = answer.expect("no flat covers an IR tiled unit").unwrap_or_else(|e| panic!("{unit:?} is answered: {e}"));
            kaspa_consensus_core::palw_da_rcore_v1::palw_da_answer_object_v1(
                &h64(999),
                claim.id,
                *unit,
                answer,
                bond_key(PRODUCER),
                court().max_close_bytes(),
                |_, _| Some(vec![2; 16]),
            )
            .unwrap_or_else(|e| panic!("{unit:?}'s answer rides: {e}"))
        })
        .collect()
}

/// **The node answers every IR step unit from its capture — dense or a fold it re-derives** (the IR
/// responder, `TirBackendV1::step_unit_answer`, the duty loop's `TirStepLeaf`/`TirStepNode` arm):
/// every leaf of the job, for the tiled and the flat scheme, is answered with a disclosure the fold's
/// own check takes (program put back), the row pin riding exactly at a logits tile of a decode row, and
/// read back as an annex it is the served annex of that leaf, byte for byte; interior nodes at every
/// level (the root, the first two and the last of each level) by a frontier and opening the fold's
/// check takes; a unit past the execution — a leaf at its leaf count, a node past its level or above
/// the root — by the claim's binding proving so. The tiled trace's rows tree likewise (`TirRowNode`):
/// every level's nodes and rows (a row by its tile leaves) with the claim's ids, exactly as a seat's
/// own rows tree over the same rows holds them (`tir_rows_tree_v1`, `tir_row_tile_leaves_v1`); a row
/// at the decode count, a node above the rows tree, and any rows node of a flat trace proven out of
/// range. The duty loop's door answers through the class's IR responder, and without one answers
/// with an error, never a guess.
#[test]
fn the_node_answers_every_ir_step_unit_from_its_capture_dense_or_fold() {
    use kaspa_consensus_core::palw_da_rcore_v1::{PalwDaAnswerV1, PalwDaUnitV1};
    use kaspa_consensus_core::palw_tir_court_v1::{
        check_tir_row_node_disclosure_v1, check_tir_step_leaf_disclosure_v1, check_tir_step_node_disclosure_v1,
        check_tir_step_out_of_range_v1, palw_tir_step_tree_height_v1, palw_tir_step_tree_width_v1,
    };
    use misaka_palw_sdk::lineages::tir::{PalwTirLeafAnnexV1, TirCaptureV1, tir_row_tile_leaves_v1, tir_rows_tree_v1};
    for tiled in [true, false] {
        let ir = ir_class(if tiled { "c-answer-tiled" } else { "c-answer-flat" }, tiled);
        let tir = ir.tir();
        let (record, _) =
            kaspa_consensus_core::palw_tir_admission_v1::palw_tir_class_record_v1(tir.class(), &ir.root).expect("the class's record");
        let program = tir.class().program.clone();
        for fold in [false, true] {
            let claim = produce_with(&ir, 21, None, fold);
            let capture = TirCaptureV1::decode(&claim.material).expect("an IR capture");
            assert_eq!(capture.is_dense(), !fold, "tiled {tiled}: the capture's kind");
            let (trace_root, execution_root) = (claim.env.attempt.trace_root, claim.env.attempt.execution_root);
            let n = capture.binding.step_leaf_count;
            let answer = |unit: PalwDaUnitV1| tir.step_unit_answer(&claim.material, unit).unwrap_or_else(|e| panic!("{unit:?}: {e}"));
            let mut pinned = 0;
            for index in 0..n {
                let PalwDaAnswerV1::TirStepLeaf(disclosure) = answer(PalwDaUnitV1::TirStepLeaf { index }) else {
                    panic!("leaf {index}: a leaf disclosure")
                };
                assert!(disclosure.binding.class.program.is_empty(), "it rides without the program");
                let filled = disclosure.with_program_v1(&record).expect("no program carried");
                check_tir_step_leaf_disclosure_v1(trace_root, execution_root, index, &filled, LADDER)
                    .unwrap_or_else(|e| panic!("tiled {tiled} fold {fold}: leaf {index} answers: {e}"));
                pinned += usize::from(disclosure.row_pin.is_some());
                let annex = PalwTirLeafAnnexV1::from_step_leaf_disclosure_v1(&disclosure).expect("read as an annex");
                assert_eq!(annex, tir.leaf_annex(&claim.material, index).expect("the served annex"), "leaf {index}: one set of bytes");
            }
            assert_eq!(pinned > 0, tiled, "a row pin rides at the logits tiles of a tiled class only ({pinned})");
            let height = palw_tir_step_tree_height_v1(n);
            assert!(height >= 1, "{n} leaves");
            for level in 1..=height {
                let width = palw_tir_step_tree_width_v1(n, level).expect("a level of the tree");
                for index in [0, 1, width - 1].into_iter().filter(|i| *i < width) {
                    let PalwDaAnswerV1::TirStepNode(disclosure) = answer(PalwDaUnitV1::TirStepNode { level, index }) else {
                        panic!("({level}, {index}): a node disclosure")
                    };
                    assert!(disclosure.binding.class.program.is_empty(), "it rides without the program");
                    let mut filled = disclosure.as_ref().clone();
                    filled.binding.class.program = program.clone();
                    check_tir_step_node_disclosure_v1(trace_root, execution_root, level, index, &filled, LADDER)
                        .unwrap_or_else(|e| panic!("tiled {tiled} fold {fold}: ({level}, {index}) answers: {e}"));
                }
                let past = PalwDaUnitV1::TirStepNode { level, index: width };
                let PalwDaAnswerV1::TirStepOutOfRange(proof) = answer(past) else { panic!("{past:?}: out of range") };
                let mut proof = proof.as_ref().clone();
                assert!(proof.class.program.is_empty());
                proof.class.program = program.clone();
                check_tir_step_out_of_range_v1(trace_root, execution_root, &past, &proof, LADDER).expect("proven past the level");
            }
            for past in [PalwDaUnitV1::TirStepLeaf { index: n }, PalwDaUnitV1::TirStepNode { level: height + 1, index: 0 }] {
                let PalwDaAnswerV1::TirStepOutOfRange(proof) = answer(past) else { panic!("{past:?}: out of range") };
                let mut proof = proof.as_ref().clone();
                proof.class.program = program.clone();
                check_tir_step_out_of_range_v1(trace_root, execution_root, &past, &proof, LADDER).expect("proven past the execution");
            }
            // The rows tree of the committed trace, as the seat's own rows tree holds it.
            let ctx = &capture.binding.job_context;
            let rows = u64::from(ctx.exact_decode_tokens);
            let rows_height = palw_tir_step_tree_height_v1(rows);
            let rows_tree = tir_rows_tree_v1(ctx, &capture.logits_rows).expect("the rows build a tree");
            let proven_out = |unit: PalwDaUnitV1| {
                let PalwDaAnswerV1::TirStepOutOfRange(proof) = answer(unit) else { panic!("tiled {tiled}: {unit:?} is out of range") };
                let mut proof = proof.as_ref().clone();
                proof.class.program = program.clone();
                check_tir_step_out_of_range_v1(trace_root, execution_root, &unit, &proof, LADDER)
                    .unwrap_or_else(|e| panic!("tiled {tiled}: {unit:?} is proven out of range: {e}"));
            };
            for level in 0..=rows_height {
                let width = palw_tir_step_tree_width_v1(rows, level).expect("a level of the rows tree");
                for index in [0, 1, width - 1].into_iter().filter(|i| *i < width) {
                    let unit = PalwDaUnitV1::TirRowNode { level, index };
                    if !tiled {
                        proven_out(unit);
                        continue;
                    }
                    let PalwDaAnswerV1::TirRowNode(disclosure) = answer(unit) else { panic!("{unit:?}: a rows-tree disclosure") };
                    assert!(disclosure.binding.class.program.is_empty(), "it rides without the program");
                    let mut filled = disclosure.as_ref().clone();
                    filled.binding.class.program = program.clone();
                    check_tir_row_node_disclosure_v1(trace_root, execution_root, level, index, &filled, LADDER)
                        .unwrap_or_else(|e| panic!("fold {fold}: {unit:?} answers: {e}"));
                    assert_eq!(filled.generated_token_ids, capture.generated, "the ids that tie the rows root to the trace root");
                    let own = if level == 0 {
                        tir_row_tile_leaves_v1(ctx, &capture.logits_rows, index as u32).expect("a row")
                    } else {
                        rows_tree.node_parts(level, index).expect("a node of the rows tree").0
                    };
                    assert_eq!(filled.frontier, own, "{unit:?}: the answer's frontier is the seat's own rows tree's");
                }
            }
            proven_out(PalwDaUnitV1::TirRowNode { level: 0, index: rows });
            proven_out(PalwDaUnitV1::TirRowNode { level: rows_height + 1, index: 0 });
            // The duty loop's door: the class's IR responder answers; none, an error.
            let units = [
                PalwDaUnitV1::TirStepNode { level: height, index: 0 },
                PalwDaUnitV1::TirStepLeaf { index: n - 1 },
                PalwDaUnitV1::TirStepLeaf { index: n },
                PalwDaUnitV1::TirRowNode { level: 0, index: rows - 1 },
            ];
            let backend = ir.backend();
            let answers = |tir: Option<&TirBackendV1>| {
                super::palw_da_claim_answers_v1(
                    backend.as_ref(),
                    &da_facts(&ir, &claim),
                    vec![claim.material.clone()],
                    |_| {},
                    &units,
                    3,
                    false,
                    tir,
                )
                .expect("the kept capture")
                .answers
            };
            let with = answers(Some(&tir));
            assert!(matches!(&with[0], Some(Ok(PalwDaAnswerV1::TirStepNode(_)))), "{:?}", with[0]);
            assert!(matches!(&with[1], Some(Ok(PalwDaAnswerV1::TirStepLeaf(d))) if d.opening.leaf_index == n - 1));
            assert!(matches!(&with[2], Some(Ok(PalwDaAnswerV1::TirStepOutOfRange(_)))), "a leaf past the job, proven so");
            assert!(
                matches!(
                    (&with[3], tiled),
                    (Some(Ok(PalwDaAnswerV1::TirRowNode(_))), true) | (Some(Ok(PalwDaAnswerV1::TirStepOutOfRange(_))), false)
                ),
                "the last row, or a flat trace's out-of-range proof: {:?}",
                with[3]
            );
            assert!(answers(None).iter().all(|a| matches!(a, Some(Err(why)) if why.contains("does not resolve"))));
        }
    }
}

/// **A demanded IR step unit through the chain** (the second IR fence, evidence transport C): the
/// seat's demand — the ONE builder, keyed by the claim alone (no binding, no draws), signed — opens an
/// R-core+ session over exactly the unit; the executor's node answers it from its kept capture (a fold
/// it re-derives, here) and the session closes with the claim standing — for the root node, a leaf,
/// and a leaf past the execution (answered by the binding proving so). An executor that stays silent
/// past `W_disclose` defaults: the claim is voided.
#[test]
fn a_demanded_ir_step_unit_is_answered_by_the_node_and_a_silent_executor_is_voided() {
    use kaspa_consensus_core::palw_da_rcore_v1::{PalwDaUnitV1, palw_tir_step_accusation_object_v1};
    use kaspa_consensus_core::palw_tir_court_v1::palw_tir_step_tree_height_v1;
    let ir = ir_class("c-chain", true);
    let honest = produce_with(&ir, 22, None, true);
    let bound = bound_c(&ir, &honest);
    let n = misaka_palw_sdk::lineages::tir::TirCaptureV1::decode(&honest.material).unwrap().binding.step_leaf_count;
    let root = PalwDaUnitV1::TirStepNode { level: palw_tir_step_tree_height_v1(n), index: 0 };
    let demand = |unit: PalwDaUnitV1| {
        palw_tir_step_accusation_object_v1(&h64(999), honest.id, unit, bond_key(SEAT), DEMAND_LADDER, |_, _| Some(vec![3; 16]))
            .expect("the demand builds")
    };
    let PalwConsensusObjectV2::DefaultAccusedTirStep { accusation } = demand(root) else { panic!("a step demand") };
    assert!(accusation.unit == root && !accusation.signature.is_empty(), "keyed by the claim alone, signed");
    let mut s = bound;
    let mut daa = C_FENCE2;
    let mut first_deadline = None;
    for unit in [root, PalwDaUnitV1::TirStepLeaf { index: n / 2 }, PalwDaUnitV1::TirStepLeaf { index: n }] {
        s = step_c(&s, daa, &[demand(unit)], None).unwrap_or_else(|e| panic!("{unit:?}: the demand opens a session: {e}"));
        let (units, deadline) = session_c(&s, &honest.id, SEAT).expect("a session is open");
        assert_eq!(units, vec![unit], "exactly the demanded unit: no draws");
        first_deadline.get_or_insert((s.clone(), deadline));
        daa += 1;
        s = step_c(&s, daa, &node_answers_c(&ir, &honest, &units), None).expect("the answer folds");
        assert!(session_c(&s, &honest.id, SEAT).is_none(), "{unit:?}: the answer refutes the session and closes it");
        assert!(matches!(phase_of(&s, &honest.id), PalwClaimPhaseV2::PanelBound { .. }), "the claim stands");
        daa += 1;
    }
    // Silent past the window on the root's demand: the claim is voided.
    let (opened, deadline) = first_deadline.expect("the root's session");
    let silent = step_c(&opened, deadline + 1, &[], None).expect("the deadline passes");
    assert!(matches!(phase_of(&silent, &honest.id), PalwClaimPhaseV2::Voided { .. }), "{:?}", phase_of(&silent, &honest.id));
    assert!(session_c(&silent, &honest.id, SEAT).is_none(), "the session closed with the default");
}

/// **A liar's capture with more leaves garbled after its lie** — the step leaves at `after` changed
/// in one byte each and the commitment re-derived over them (step root, execution root, the attempt),
/// so no seat can rebuild the accused's tree as "mine with one leaf replaced": what it knows of that
/// tree past the disputed leaf is the leaf's disclosed opening, and nothing else.
fn garbled(ir: &Ir, liar: &Claim, after: &[u64]) -> Claim {
    use kaspa_consensus_core::palw_step_leg::{step_merkle_root_v1, step_tile_leaf_hash_v1};
    let mut capture = misaka_palw_sdk::lineages::tir::TirCaptureV1::decode(&liar.material).expect("an IR capture");
    assert!(capture.is_dense(), "a dense capture to garble");
    for &i in after {
        let leaf = &mut capture.leaves[i as usize];
        leaf.values_le[0] ^= 0x40;
    }
    let ctx = capture.binding.job_context.clone();
    let hashes: Vec<Hash64> = capture.leaves.iter().map(|p| step_tile_leaf_hash_v1(&ctx.context_hash(), &ir.class_id, p)).collect();
    let b = &mut capture.binding;
    b.step_merkle_root = step_merkle_root_v1(&hashes).expect("a step root");
    b.committed_execution_root = kaspa_consensus_core::palw_tir_step_v1::palw_tir_execution_root_v1(
        &ctx.context_hash(),
        &b.full_logits_trace_root,
        &ir.class_id,
        b.step_leaf_count,
        &b.step_merkle_root,
    );
    let mut env = liar.env.clone();
    env.attempt.execution_root = b.committed_execution_root;
    let id = attempt_id_v2(&env.attempt);
    Claim { env, id, job: liar.job.clone(), prompt: liar.prompt.clone(), material: capture.encode() }
}

/// **A liar that serves no annex is made to disclose on chain and is convicted from what it
/// disclosed** (evidence transport C, the seats' half), against a liar that garbles leaves after its
/// lie too: the first seat demands the root, then the first frontier node its own tree disputes, eight
/// levels a session, then the leaf — each answered by the liar's own node from its capture and read
/// back off the chain (`palw_tir_pursuit_absorb_chain_v1`, `palw_tir_pursuit_descend_known_nodes_v1`)
/// — in ⌈h/8⌉ + 1 sessions; the second seat reads the same chain, waits on every open demand, spends
/// no session and reaches the same leaf. Each builds the cone close from its own execution below the
/// leaf and the leaf's disclosure (the accused's tree past the leaf is only its opening), the gate
/// reads it `ExecutorGuilty`, and the fold voids the claim for fraud. Silent on the first demand, the
/// liar is voided at the deadline instead.
#[test]
fn a_withholding_liar_is_demanded_on_chain_and_convicted_from_its_disclosure() {
    use super::tir_court::{
        PalwTirAnnexPursuitV1, PalwTirAnnexStepV1, PalwTirWithheldStepV1, palw_tir_annex_step_v1, palw_tir_leaf_is_dissected_v1,
        palw_tir_one_move_accusation_to_file_v1, palw_tir_one_move_verdict_stateless_v1, palw_tir_pursuit_absorb_chain_v1,
        palw_tir_pursuit_descend_known_nodes_v1, palw_tir_withheld_step_v1, palw_tir_withheld_unit_v1,
    };
    use kaspa_consensus_core::palw_da_rcore_v1::{PalwDaUnitV1, palw_tir_step_accusation_object_v1};
    use kaspa_consensus_core::palw_producer_v2::PalwDisputableClaimV2;
    use kaspa_consensus_core::palw_tir_court_v1::palw_tir_step_tree_height_v1;
    use kaspa_consensus_core::palw_tir_step_v1::PalwTirLeafKindV1;
    let ir = ir_class("c-liar", true);
    let tir = ir.tir();
    let rules = tir.court_rules(&court());
    let program = tir.class().program.clone();
    let honest = produce(&ir, 23, None);
    let ctx = misaka_palw_sdk::lineages::tir::TirCaptureV1::decode(&honest.material).unwrap().binding.job_context;
    let prompt: Vec<u32> = honest.prompt.iter().map(|t| *t as u32).collect();
    let own = tir.retain_memo(&ctx, &prompt).expect("the seat's own run");
    let n = own.leaf_hashes.len() as u64;
    let height = palw_tir_step_tree_height_v1(n);
    // A planted lie deep in the tree at an undissected commit leaf, and two leaves after it garbled.
    let lie = (n / 2..n - 2)
        .find(|&leaf| {
            !palw_tir_leaf_is_dissected_v1(&tir, &ctx, leaf)
                && tir.space().leaf_at(&ctx, leaf).is_some_and(|l| matches!(l.kind, PalwTirLeafKindV1::Commit { .. }))
        })
        .expect("an undissected commit leaf");
    let liar = garbled(&ir, &produce(&ir, 23, Some(lie)), &[lie + 1, n - 1]);
    let target = PalwDisputableClaimV2 {
        accepted_block: Default::default(),
        claim_id: liar.id,
        class_id: ir.class_id,
        artifact_root: ir.root,
        executor_bond: bond_key(PRODUCER),
        trace_root: liar.env.attempt.trace_root,
        execution_root: liar.env.attempt.execution_root,
        licensed_daa: 102,
        free_prompt: false,
    };
    let window = 20; // params()'s challenge window: W_disclose
    let mut s = bound_c(&ir, &liar);
    let mut daa = C_FENCE2;
    // Two seats of the panel, whose executor served neither an annex: SEAT demands, COLLUDER reads the
    // chain — every demand and answer folded — and waits on SEAT's demands instead of filing its own.
    let (mut first, mut second) = (PalwTirAnnexPursuitV1::new(own.clone(), daa), PalwTirAnnexPursuitV1::new(own.clone(), daa));
    for p in [&mut first, &mut second] {
        p.asks = super::tir_court::PALW_TIR_ANNEX_ASKS_V1;
        p.withheld = true;
    }
    assert_eq!(palw_tir_withheld_unit_v1(&first), PalwDaUnitV1::TirStepNode { level: height, index: 0 }, "the root first");
    let mut chain: Vec<PalwConsensusObjectV2> = Vec::new();
    // The chain's objects into a pursuit, the known nodes walked, then its steps on a disclosed leaf.
    let advance = |p: &mut PalwTirAnnexPursuitV1, me: u64, chain: &[PalwConsensusObjectV2], daa: u64| -> Option<PalwTirAnnexStepV1> {
        palw_tir_pursuit_absorb_chain_v1(p, chain, &bond_key(me), &program, &target, LADDER, daa);
        loop {
            palw_tir_pursuit_descend_known_nodes_v1(p);
            let annex = p.disclosed.get(&p.leaf).cloned()?;
            let binding = p.binding.as_deref().expect("a disclosed leaf shows the binding").clone();
            match palw_tir_annex_step_v1(&tir, p, &annex, &binding, &rules) {
                PalwTirAnnexStepV1::Ask { leaf, below, token } => {
                    (p.leaf, p.below, p.token) = (leaf, below, token);
                    p.rounds += 1;
                }
                end => return Some(end),
            }
        }
    };
    let mut silent_checked = false;
    let end = loop {
        assert!(daa < C_FENCE2 + 200, "the pursuit ends");
        if let Some(end) = advance(&mut first, SEAT, &chain, daa) {
            break end;
        }
        let unit = match palw_tir_withheld_step_v1(true, &first, daa, window, 4, 0) {
            PalwTirWithheldStepV1::Demand { unit } => unit,
            other => panic!("the pursuit stalls at {:?}: {other:?}", palw_tir_withheld_unit_v1(&first)),
        };
        first.demanded = Some((unit, daa));
        first.sessions += 1;
        let object =
            palw_tir_step_accusation_object_v1(&h64(999), liar.id, unit, bond_key(SEAT), DEMAND_LADDER, |_, _| Some(vec![3; 16]))
                .expect("the demand");
        s = step_c(&s, daa, std::slice::from_ref(&object), None)
            .unwrap_or_else(|e| panic!("DAA {daa}: {unit:?} opens a session: {e}"));
        chain.push(object);
        // COLLUDER reads SEAT's demand pending and waits for its answer, whatever its own stagger.
        if advance(&mut second, COLLUDER, &chain, daa).is_none() {
            assert_eq!(palw_tir_withheld_unit_v1(&second), unit, "both seats' descents stand at the same unit");
            second.withheld_since.get_or_insert((unit, daa));
            assert_eq!(
                palw_tir_withheld_step_v1(true, &second, daa + 10, window, 4, 0),
                PalwTirWithheldStepV1::Wait,
                "the other seat waits on the open demand of {unit:?}"
            );
        }
        let (units, deadline) = session_c(&s, &liar.id, SEAT).expect("a session");
        assert_eq!(units, vec![unit], "exactly the demanded unit");
        // Once, on the first demand: the liar silent past the window is voided.
        if !silent_checked {
            let silent = step_c(&s, deadline + 1, &[], None).expect("the deadline passes");
            assert!(matches!(phase_of(&silent, &liar.id), PalwClaimPhaseV2::Voided { .. }), "a silent liar defaults");
            silent_checked = true;
        }
        // The liar's node answers from its capture (the lie and the garbled leaves included).
        let answers = node_answers_c(&ir, &liar, &units);
        daa += 1;
        s = step_c(&s, daa, &answers, None).unwrap_or_else(|e| panic!("DAA {daa}: the answer folds: {e}"));
        assert!(session_c(&s, &liar.id, SEAT).is_none(), "answered: the session closes");
        chain.extend(answers);
        daa += 1;
    };
    assert!(silent_checked);
    let expected = u32::from(height).div_ceil(8) + 1;
    assert_eq!(u32::from(first.sessions), expected, "{height} levels: ⌈h/8⌉ node sessions and the leaf");
    assert!(first.sessions <= 4, "inside one seat's four sessions");
    let other_end = advance(&mut second, COLLUDER, &chain, daa).expect("the other seat's descent ends on the same answers");
    assert_eq!(second.sessions, 0, "the other seat spent no session of its own");
    let mut filed = None;
    for (who, accuser, end) in [("SEAT", SEAT, end), ("COLLUDER", COLLUDER, other_end)] {
        let PalwTirAnnexStepV1::Accuse { leaf, candidates, .. } = end else { panic!("{who}: an accusation, got another end") };
        assert_eq!(leaf, Some(lie), "{who}: the chain's disclosures lead the descent to the planted leaf");
        let (label, accusation) =
            palw_tir_one_move_accusation_to_file_v1(candidates, &target, &program, bond_key(accuser), &court(), LADDER, FORM)
                .expect("an accusation to file");
        assert_eq!(label, "cone");
        assert_eq!(
            palw_tir_one_move_verdict_stateless_v1(&accusation.proof, &target, &program, &court(), LADDER, FORM),
            Some(PalwCourtVerdictV2::ExecutorGuilty),
            "{who}: the cone close built from the disclosure convicts"
        );
        filed.get_or_insert(accusation);
    }
    // It rides as a one-move accusation, and the fold voids the claim for fraud.
    let mut accusation = filed.expect("SEAT's accusation");
    accusation.signature = vec![9; 16];
    let voided = step_c(&s, daa, &[PalwConsensusObjectV2::TirShardCourtAccused { accusation: Box::new(accusation) }], None)
        .expect("the accusation folds");
    assert!(
        matches!(
            phase_of(&voided, &liar.id),
            PalwClaimPhaseV2::Voided { reason: kaspa_consensus_core::palw_state_v2::PalwVoidReasonV2::CourtFraud, .. }
        ),
        "the forged claim is voided for fraud: {:?}",
        phase_of(&voided, &liar.id)
    );
}

/// **The withheld pursuit's decisions** (`palw_tir_withheld_step_v1`, `palw_tir_node_descent_v1`),
/// and the wiring: the tick hands a withheld claim to the demand tick, which reads the chain, walks
/// the known nodes, staggers, and rehearses each demand on the tip before queueing it; the duty loop
/// resolves the IR responder for a step unit.
#[test]
fn a_withheld_pursuit_demands_on_chain_only_past_the_fence_and_within_its_sessions() {
    use super::tir_court::{
        PalwTirAnnexPursuitV1, PalwTirWithheldStepV1 as W, palw_tir_demand_stagger_v1, palw_tir_node_descent_v1,
        palw_tir_withheld_step_v1 as step, palw_tir_withheld_unit_v1 as unit_of,
    };
    use kaspa_consensus_core::palw_da_rcore_v1::PalwDaUnitV1;
    use kaspa_consensus_core::palw_tir_court_v1::{palw_tir_step_node_parts_v1, palw_tir_step_tree_height_v1};
    use misaka_palw_sdk::lineages::tir::TirStepTreeV1;
    let ir = ir_class("c-decide", true);
    let tir = ir.tir();
    let honest = produce(&ir, 24, None);
    let ctx = misaka_palw_sdk::lineages::tir::TirCaptureV1::decode(&honest.material).unwrap().binding.job_context;
    let prompt: Vec<u32> = honest.prompt.iter().map(|t| *t as u32).collect();
    let own = tir.retain_memo(&ctx, &prompt).expect("the seat's own run");
    let n = own.leaf_hashes.len() as u64;
    let height = palw_tir_step_tree_height_v1(n);
    let root = PalwDaUnitV1::TirStepNode { level: height, index: 0 };
    let (window, max) = (20, 4);
    let mut p = PalwTirAnnexPursuitV1::new(own.clone(), 100);
    assert_eq!(unit_of(&p), root, "the descent starts at the root");
    assert!(matches!(step(false, &p, 100, window, max, 0), W::GiveUp(_)), "below the fence");
    assert_eq!(step(true, &p, 100, window, max, 0), W::Demand { unit: root });
    // The stagger, counted from the round's start.
    p.withheld_since = Some((root, 100));
    assert_eq!(step(true, &p, 105, window, max, 9), W::Wait, "inside this seat's slot");
    assert_eq!(step(true, &p, 109, window, max, 9), W::Demand { unit: root }, "at its end");
    // Another bond's demand of the unit, read off the chain, is waited on — its answer is every seat's.
    p.pending.insert(root, 104);
    assert_eq!(step(true, &p, 109, window, max, 9), W::Wait, "another's demand is open");
    assert_eq!(step(true, &p, 104 + window + 5, window, max, 9), W::Demand { unit: root }, "past its window, unanswered");
    p.pending.clear();
    p.demanded = Some((root, 110));
    assert_eq!(step(true, &p, 110 + window, window, max, 0), W::Wait, "this seat's own demand");
    // The descent over frontiers, against an accused that differs at leaf `lie` and after it.
    let lie = n / 2 + 1;
    let mut accused = own.leaf_hashes.clone();
    for i in [lie, lie + 1, n - 1] {
        accused[i as usize] = Hash64::from_u64_word(0xBAD0 + i);
    }
    let mut at = (height, 0u64);
    let mut rounds = 0;
    let found = loop {
        rounds += 1;
        let (frontier, _) = palw_tir_step_node_parts_v1(&accused, at.0, at.1).expect("the accused's node");
        let (leaf, below) =
            palw_tir_node_descent_v1(&TirStepTreeV1::full(&own.leaf_hashes), at.0, at.1, &frontier).expect("a difference");
        if below == 0 {
            break leaf;
        }
        at = (below as u8, leaf >> below);
    };
    assert_eq!(found, lie, "the first differing leaf, whatever differs after it");
    assert_eq!(rounds, u32::from(height).div_ceil(8), "{height} levels: eight a session");
    let (frontier, _) = palw_tir_step_node_parts_v1(&own.leaf_hashes, height, 0).expect("the own root");
    assert!(palw_tir_node_descent_v1(&TirStepTreeV1::full(&own.leaf_hashes), height, 0, &frontier).is_none(), "an agreeing root");
    // Positions map to units: a subtree to its node, level 0 and the decode-token door to the leaf.
    p.leaf = lie;
    p.below = Some(0);
    assert_eq!(unit_of(&p), PalwDaUnitV1::TirStepLeaf { index: lie });
    p.below = Some(3);
    p.leaf = (lie >> 3) << 3;
    assert_eq!(unit_of(&p), PalwDaUnitV1::TirStepNode { level: 3, index: lie >> 3 });
    p.token = Some((0, 7));
    assert_eq!(unit_of(&p), PalwDaUnitV1::TirStepLeaf { index: (lie >> 3) << 3 });
    p.token = None;
    // Sessions spent: wait while another's demand of the unit is open, give up when none is.
    let unit = unit_of(&p);
    p.sessions = 4;
    p.pending.insert(unit, 138);
    assert_eq!(step(true, &p, 140, window, max, 0), W::Wait, "sessions spent, but another's demand of the unit is open");
    p.pending.clear();
    assert!(matches!(step(true, &p, 140, window, max, 0), W::GiveUp(_)), "a seat's four sessions spent, nothing open");
    // Seats draw different slots for one unit, and one seat different slots across units.
    let slots: std::collections::BTreeSet<u64> =
        (0..16u64).map(|b| palw_tir_demand_stagger_v1(&bond_key(b), &honest.id, &root)).collect();
    assert!(slots.len() > 3 && slots.iter().all(|s| s % 3 == 0 && *s < 24), "{slots:?}");
    let units: std::collections::BTreeSet<u64> =
        (0..16u64).map(|i| palw_tir_demand_stagger_v1(&bond_key(SEAT), &honest.id, &PalwDaUnitV1::TirStepLeaf { index: i })).collect();
    assert!(units.len() > 3, "{units:?}");

    let court_src = include_str!("tir_court.rs");
    let court_src = &court_src[..court_src.find("\n#[cfg(test)]").unwrap_or(court_src.len())];
    let tick = &court_src[court_src.find("    async fn tir_annex_pursuit_tick_v1(").expect("the tick")..];
    assert!(tick.contains(
        "self.tir_demand_tick_v1(session, bond_key, network_domain, current_daa, target, due, tir, &program, ladder, books).await;"
    ));
    let demand = &court_src[court_src.find("    async fn tir_demand_tick_v1(").expect("the demand tick")..];
    let demand = &demand[..demand.find("\n    }\n").expect("its end")];
    for reached in [
        "tir_da_chain_objects_v1(c, claim, not_before, span)",
        "palw_tir_pursuit_absorb_chain_v1(p, &objects, &bond_key, program, target, ladder, current_daa);",
        "palw_tir_pursuit_descend_known_nodes_v1(p)",
        "let stagger = palw_tir_demand_stagger_v1(&bond_key, &claim, &unit);",
        "palw_tir_step_accusation_object_v1(",
        "self.file_tir_demand_v1(session, &object, palw_tir_demand_queue_key_v1(message), due, books)",
    ] {
        assert!(demand.contains(reached), "the demand tick reaches {reached}");
    }
    let file = &court_src[court_src.find("    fn file_tir_demand_v1(").expect("the filer")..];
    assert!(file.find("session.palw_object_rehearsal_v1(object)").unwrap() < file.find("books.court_pending.push(").unwrap());
    let panel = include_str!("../palw_panel.rs");
    let rcore = &panel[panel.find("    async fn rcore_da_answers_v1(").expect("the answers")..];
    assert!(rcore.contains("units.iter().any(|unit| unit.is_tir_fence2_v1())") && rcore.contains("tir.as_ref(),"));
    let unit = &panel[panel.find("pub(crate) fn palw_da_unit_answer_v1(").expect("the unit answer")..];
    assert!(unit.contains("return tir.step_unit_answer(capture, unit);"));
}

// ---- the trace's descent: RowNode (the second IR fence), the seats' half --------------------------

/// The claim as a seat's one-move pass reads a target (the disputable-claim view).
fn target_c(ir: &Ir, claim: &Claim) -> kaspa_consensus_core::palw_producer_v2::PalwDisputableClaimV2 {
    kaspa_consensus_core::palw_producer_v2::PalwDisputableClaimV2 {
        accepted_block: Default::default(),
        claim_id: claim.id,
        class_id: ir.class_id,
        artifact_root: ir.root,
        executor_bond: bond_key(PRODUCER),
        trace_root: claim.env.attempt.trace_root,
        execution_root: claim.env.attempt.execution_root,
        licensed_daa: 102,
        free_prompt: false,
    }
}

/// A close as it rides: its binding's program emptied.
fn as_filed_c(built: Result<PalwCourtVerdictProofV2, String>) -> Result<PalwCourtVerdictProofV2, String> {
    built.map(|mut proof| {
        proof.tir_strip_program_v1();
        proof
    })
}

/// The seat's own run of `claim`'s job.
fn own_run(tir: &TirBackendV1, claim: &Claim) -> std::sync::Arc<misaka_palw_sdk::lineages::tir::TirRetainedJobV1> {
    let prompt: Vec<u32> = claim.prompt.iter().map(|t| *t as u32).collect();
    tir.retain_memo(&claim.job, &prompt).expect("the seat's own run")
}

/// **A trace pursuit on the chain's path**, as the demand tick holds it: the trace's descent over the
/// seat's own rows tree, the served annexes' door token cleared (on chain the rows root comes first).
fn trace_pursuit_c(
    tir: &TirBackendV1,
    own: &std::sync::Arc<misaka_palw_sdk::lineages::tir::TirRetainedJobV1>,
) -> super::tir_court::PalwTirAnnexPursuitV1 {
    let ctx = &own.binding.job_context;
    let rows = misaka_palw_sdk::lineages::tir::tir_rows_tree_v1(ctx, &own.logits_rows).expect("the seat's rows tree");
    let row0 = tir.logits_leaf_holding(ctx, 0, u64::from(own.generated[0]));
    let mut p = super::tir_court::PalwTirAnnexPursuitV1::new(own.clone(), C_FENCE2).with_trace(rows, row0);
    (p.token, p.withheld) = (None, true);
    p
}

/// **Option C's rounds, driven to an end**: the seat `me`'s pursuit reads the chain (every demand and
/// answer folded so far, `palw_tir_pursuit_absorb_chain_v1`), walks what it knows (the step tree's
/// nodes, the rows tree's), and — until `done` — demands the unit it stands at (an IR step or rows
/// demand, or an event demand by the ONE event builder), which the liar's node answers, every unit
/// of the session, from its capture. Returns the chain state and the DAA past the last answer.
#[allow(clippy::too_many_arguments)]
fn drive_c(
    ir: &Ir,
    tir: &TirBackendV1,
    liar: &Claim,
    p: &mut super::tir_court::PalwTirAnnexPursuitV1,
    me: u64,
    mut s: PalwChainStateV2,
    chain: &mut Vec<PalwConsensusObjectV2>,
    done: &dyn Fn(&super::tir_court::PalwTirAnnexPursuitV1) -> bool,
) -> (PalwChainStateV2, u64) {
    use super::tir_court::{
        PalwTirWithheldStepV1, palw_tir_pursuit_absorb_chain_v1, palw_tir_pursuit_descend_known_nodes_v1,
        palw_tir_pursuit_descend_known_rows_v1, palw_tir_withheld_step_v1,
    };
    use kaspa_consensus_core::palw_da_rcore_v1::{PalwDaUnitV1, palw_da_accusation_object_v1, palw_tir_step_accusation_object_v1};
    let program = tir.class().program.clone();
    let target = target_c(ir, liar);
    let ctx = p.own.binding.job_context.clone();
    let mut daa = C_FENCE2;
    loop {
        assert!(daa < C_FENCE2 + 200, "the descent ends");
        palw_tir_pursuit_absorb_chain_v1(p, chain, &bond_key(me), &program, &target, LADDER, daa);
        palw_tir_pursuit_descend_known_nodes_v1(p);
        palw_tir_pursuit_descend_known_rows_v1(p, &|row, token| tir.logits_leaf_holding(&ctx, row, u64::from(token)));
        if done(p) {
            return (s, daa);
        }
        let unit = match palw_tir_withheld_step_v1(true, p, daa, 20, 4, 0) {
            PalwTirWithheldStepV1::Demand { unit } => unit,
            other => panic!("the descent stalls: {other:?}"),
        };
        p.demanded = Some((unit, daa));
        p.sessions += 1;
        let sign = |_: &[u8], _: &[u8]| Some(vec![3; 16]);
        let object = match unit {
            PalwDaUnitV1::Event { .. } => palw_da_accusation_object_v1(&h64(999), liar.id, unit, bond_key(me), sign),
            _ => palw_tir_step_accusation_object_v1(&h64(999), liar.id, unit, bond_key(me), DEMAND_LADDER, sign),
        }
        .unwrap_or_else(|e| panic!("{unit:?}: the demand builds: {e}"));
        s = step_c(&s, daa, std::slice::from_ref(&object), None)
            .unwrap_or_else(|e| panic!("DAA {daa}: {unit:?} opens a session: {e}"));
        chain.push(object);
        let (units, _) = session_c(&s, &liar.id, me).expect("a session");
        assert!(units.contains(&unit), "{unit:?} is demanded: {units:?}");
        let answers = node_answers_c(ir, liar, &units);
        daa += 1;
        s = step_c(&s, daa, &answers, None).unwrap_or_else(|e| panic!("DAA {daa}: the answers fold: {e}"));
        assert!(session_c(&s, &liar.id, me).is_none(), "{unit:?}: answered, the session closes");
        chain.extend(answers);
        daa += 1;
    }
}

/// **A liar whose lie is only in its trace rows is found down its rows tree and convicted by one
/// seat** (the second IR fence's `TirRowNode`, the seats' half): the liar commits the honest steps and
/// ids and a trace whose one row is moved at one lane under the row's maximum (the selection stands).
/// The seat's replay does not reproduce the claim, but its own step root beside the claim's trace root
/// gives the claim's execution root (`palw_tir_steps_agree_v1`): the lie is in the trace. On chain it
/// demands the rows root, then the row its own rows tree disputes (its tile leaves), then that tile's
/// lanes as an event unit — each answered by the liar's own node from its capture and read back off
/// the chain — `⌈h/8⌉ + 2` sessions, inside its four; a second seat reading the same chain reaches the
/// same tile without a session of its own. The seat's own step tile beside the disclosed one is the
/// logits door (`TirBackendV1::trace_logits_close`), the gate reads it `ExecutorGuilty`, and the fold
/// voids the claim for fraud. Against the honest trace the same close finds nothing.
#[test]
fn a_trace_only_liar_is_found_down_its_rows_tree_and_convicted_by_one_seat() {
    use super::tir_court::{
        palw_tir_one_move_accusation_to_file_v1, palw_tir_one_move_verdict_stateless_v1, palw_tir_pursuit_absorb_chain_v1,
        palw_tir_pursuit_descend_known_rows_v1, palw_tir_steps_agree_v1, palw_tir_withheld_unit_v1,
    };
    use kaspa_consensus_core::palw_da_rcore_v1::PalwDaUnitV1;
    use kaspa_consensus_core::palw_state_v2::PalwVoidReasonV2;
    use kaspa_consensus_core::palw_step_refute::PALW_LOGITS_TILE_LANES;
    use kaspa_consensus_core::palw_tir_court_v1::palw_tir_step_tree_height_v1;
    use misaka_palw_sdk::lineages::tir::TirCaptureV1;
    let ir = ir_class("c-rows", true);
    let tir = ir.tir();
    let program = tir.class().program.clone();
    let honest = produce(&ir, 25, None);
    let capture = TirCaptureV1::decode(&honest.material).expect("an IR capture");
    let decode = capture.logits_rows.len();
    let r = decode / 2;
    let selected = capture.generated[r] as usize;
    let top = *capture.logits_rows[r].iter().max().expect("a row");
    let lane = (0..capture.logits_rows[r].len())
        .find(|l| *l != selected && capture.logits_rows[r][*l] + 1 < top)
        .expect("a lane under the row's maximum");
    let liar = craft(&ir, &honest, |c| c.logits_rows[r][lane] += 1);
    assert_eq!(TirCaptureV1::decode(&liar.material).unwrap().generated, capture.generated, "the ids stand");
    let target = target_c(&ir, &liar);
    let own = own_run(&tir, &honest);
    assert!(palw_tir_steps_agree_v1(&own, &target.trace_root, &target.execution_root), "the steps are the seat's own");
    assert!(!palw_tir_steps_agree_v1(&own, &target.trace_root, &honest.env.attempt.execution_root), "and nothing else is");
    let height = palw_tir_step_tree_height_v1(decode as u64);
    let mut first = trace_pursuit_c(&tir, &own);
    assert_eq!(palw_tir_withheld_unit_v1(&first), PalwDaUnitV1::TirRowNode { level: height, index: 0 }, "the rows root first");
    let mut chain = Vec::new();
    let reached = |p: &super::tir_court::PalwTirAnnexPursuitV1| {
        p.trace.as_ref().and_then(|t| t.event).is_some_and(|e| p.events.contains_key(&e))
    };
    let (s, daa) = drive_c(&ir, &tir, &liar, &mut first, SEAT, bound_c(&ir, &liar), &mut chain, &reached);
    let (row, tile) = first.trace.as_ref().and_then(|t| t.event).expect("the event reached");
    assert_eq!((row as usize, usize::from(tile)), (r, lane / PALW_LOGITS_TILE_LANES), "the forged row, and the tile of its lane");
    let expected = u32::from(height).div_ceil(8) + 2;
    assert_eq!(u32::from(first.sessions), expected, "{decode} rows ({height} levels): the rows root, the row, the tile");
    assert!(first.sessions <= 4, "inside one seat's four sessions");
    // A second seat reads the same chain and stands at the same tile, having spent nothing.
    let mut second = trace_pursuit_c(&tir, &own);
    palw_tir_pursuit_absorb_chain_v1(&mut second, &chain, &bond_key(COLLUDER), &program, &target, LADDER, daa);
    palw_tir_pursuit_descend_known_rows_v1(&mut second, &|_, _| None);
    assert_eq!(second.trace.as_ref().and_then(|t| t.event), Some((row, tile)), "the other seat reaches the same tile");
    assert_eq!(second.sessions, 0);
    // The close: the seat's own step tile beside the disclosed tile.
    let event = first.events.get(&(row, tile)).expect("the tile is disclosed").clone();
    let binding = first.binding.as_deref().expect("the claim's binding").clone();
    let proof = tir.trace_logits_close(&own, &binding, row, tile, &event).expect("the logits door");
    let (label, mut accusation) = palw_tir_one_move_accusation_to_file_v1(
        vec![("logits", as_filed_c(Ok(proof)))],
        &target,
        &program,
        bond_key(SEAT),
        &court(),
        LADDER,
        FORM,
    )
    .expect("the close convicts");
    assert_eq!(label, "logits");
    assert_eq!(
        palw_tir_one_move_verdict_stateless_v1(&accusation.proof, &target, &program, &court(), LADDER, FORM),
        Some(PalwCourtVerdictV2::ExecutorGuilty)
    );
    accusation.signature = vec![9; 16];
    let voided = step_c(&s, daa, &[PalwConsensusObjectV2::TirShardCourtAccused { accusation: Box::new(accusation) }], None)
        .expect("the accusation folds");
    assert!(
        matches!(phase_of(&voided, &liar.id), PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtFraud, .. }),
        "the trace liar is voided for fraud: {:?}",
        phase_of(&voided, &liar.id)
    );
    // The honest trace's own tile: nothing to accuse.
    let honest_event = misaka_palw_sdk::lineages::tir::tir_trace_event_disclosure_of_capture_v1(&honest.material, row, tile)
        .expect("an IR capture")
        .expect("an event of the run");
    assert!(tir.trace_logits_close(&own, &own.binding, row, tile, &honest_event).is_err(), "the honest tile is the seat's own");
}

/// **An ids-only liar is found at its rows root and convicted at the decode-token door**: the steps
/// and every trace row honest, one generated id not the greedy selection of its (honest) row. The
/// rows root's answer carries the claim's ids and a frontier that is the seat's own throughout — the
/// rows agree — so the seat's next demand is the logits leaf of the first row whose id parts that
/// holds its own token; that leaf's disclosed row pin is the door (stepped as a disclosed annex is),
/// and it convicts. Two sessions.
#[test]
fn an_ids_only_liar_is_found_at_its_rows_root_and_convicted_at_the_door() {
    use super::tir_court::{
        PalwTirAnnexStepV1, palw_tir_annex_step_v1, palw_tir_one_move_accusation_to_file_v1, palw_tir_steps_agree_v1,
    };
    use kaspa_consensus_core::palw_state_v2::PalwVoidReasonV2;
    use misaka_palw_sdk::lineages::tir::TirCaptureV1;
    let ir = ir_class("c-ids", true);
    let tir = ir.tir();
    let rules = tir.court_rules(&court());
    let program = tir.class().program.clone();
    let honest = produce(&ir, 26, None);
    let capture = TirCaptureV1::decode(&honest.material).expect("an IR capture");
    let k = capture.generated.len().min(2) - 1;
    let vocab = capture.logits_rows[k].len() as u32;
    let liar = craft(&ir, &honest, |c| c.generated[k] = (c.generated[k] + 1) % vocab);
    let target = target_c(&ir, &liar);
    let own = own_run(&tir, &honest);
    assert!(palw_tir_steps_agree_v1(&own, &target.trace_root, &target.execution_root), "the steps are the seat's own");
    let mut p = trace_pursuit_c(&tir, &own);
    let mut chain = Vec::new();
    let at_the_door = |p: &super::tir_court::PalwTirAnnexPursuitV1| p.token.is_some() && p.disclosed.contains_key(&p.leaf);
    let (s, daa) = drive_c(&ir, &tir, &liar, &mut p, SEAT, bound_c(&ir, &liar), &mut chain, &at_the_door);
    assert!(p.trace.as_ref().is_some_and(|t| t.rows_agree && t.event.is_none()), "the rows are the seat's own");
    assert_eq!(p.token, Some((k as u32, own.generated[k])), "the door at the first id that parts, its lane the seat's own token");
    assert_eq!(p.sessions, 2, "the rows root, then the door's leaf");
    let annex = p.disclosed.get(&p.leaf).expect("the door's leaf is disclosed").clone();
    let binding = p.binding.as_deref().expect("the claim's binding").clone();
    let PalwTirAnnexStepV1::Accuse { row, candidates, .. } = palw_tir_annex_step_v1(&tir, &p, &annex, &binding, &rules) else {
        panic!("the door's leaf is an accusation")
    };
    assert_eq!(row, Some(k as u32));
    let (label, mut accusation) =
        palw_tir_one_move_accusation_to_file_v1(candidates, &target, &program, bond_key(SEAT), &court(), LADDER, FORM)
            .expect("the door convicts");
    assert_eq!(label, "decode token");
    accusation.signature = vec![9; 16];
    let voided = step_c(&s, daa, &[PalwConsensusObjectV2::TirShardCourtAccused { accusation: Box::new(accusation) }], None)
        .expect("the accusation folds");
    assert!(
        matches!(phase_of(&voided, &liar.id), PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtFraud, .. }),
        "{:?}",
        phase_of(&voided, &liar.id)
    );
}

/// **Served annexes on a trace descent ask decode row 0's door leaf first** (where an ask costs no
/// session): the pursuit of a claim whose steps are the seat's own asks the logits leaf of decode row 0
/// holding its own token there; that annex carries the claim's ids and row 0's pin — the door itself
/// when row 0 holds the first id that parts, a re-aim at the first row that does otherwise — and ids
/// that are the seat's own leave the rows to the chain (`Rows`), which no annex reaches.
#[test]
fn served_annexes_on_a_trace_descent_ask_decode_row_zeros_door_leaf_first() {
    use super::tir_court::{
        PalwTirAnnexPursuitV1, PalwTirAnnexStepV1, palw_tir_annex_step_v1, palw_tir_one_move_accusation_to_file_v1,
    };
    use misaka_palw_sdk::lineages::tir::{TirCaptureV1, palw_tir_leaf_annex_verify_v1, tir_rows_tree_v1};
    let ir = ir_class("b-rows", true);
    let tir = ir.tir();
    let rules = tir.court_rules(&court());
    let program = tir.class().program.clone();
    let honest = produce(&ir, 27, None);
    let capture = TirCaptureV1::decode(&honest.material).expect("an IR capture");
    let (decode, vocab) = (capture.generated.len(), capture.logits_rows[0].len() as u32);
    let own = own_run(&tir, &honest);
    let ctx = &own.binding.job_context;
    let row0 = tir.logits_leaf_holding(ctx, 0, u64::from(own.generated[0])).expect("decode row 0's door leaf");
    let last = decode - 1;
    let r = decode / 2;
    let lane = (0..capture.logits_rows[r].len())
        .find(|l| {
            *l != capture.generated[r] as usize && capture.logits_rows[r][*l] + 1 < *capture.logits_rows[r].iter().max().unwrap()
        })
        .expect("a lane under the row's maximum");
    let mut liars = vec![
        ("row 0's id", craft(&ir, &honest, |c| c.generated[0] = (c.generated[0] + 1) % vocab)),
        ("a row's lane", craft(&ir, &honest, |c| c.logits_rows[r][lane] += 1)),
    ];
    if last > 0 {
        liars.push(("a later id", craft(&ir, &honest, |c| c.generated[last] = (c.generated[last] + 1) % vocab)));
    }
    for (what, liar) in liars {
        let target = target_c(&ir, &liar);
        let p = PalwTirAnnexPursuitV1::new(own.clone(), 100)
            .with_trace(tir_rows_tree_v1(ctx, &own.logits_rows).expect("the seat's rows tree"), Some(row0));
        assert_eq!((p.leaf, p.token), (row0, Some((0, own.generated[0]))), "{what}: decode row 0's door leaf is asked first");
        let annex = tir.leaf_annex(&liar.material, p.leaf).expect("the served annex");
        let binding = palw_tir_leaf_annex_verify_v1(&annex, &program, target.execution_root, target.trace_root, LADDER)
            .expect("the annex verifies against the claim");
        match (what, palw_tir_annex_step_v1(&tir, &p, &annex, &binding, &rules)) {
            ("row 0's id", PalwTirAnnexStepV1::Accuse { row: Some(0), candidates, .. }) => {
                let (label, _) =
                    palw_tir_one_move_accusation_to_file_v1(candidates, &target, &program, bond_key(SEAT), &court(), LADDER, FORM)
                        .expect("row 0's door convicts from the first annex");
                assert_eq!(label, "decode token");
            }
            ("a later id", PalwTirAnnexStepV1::Ask { leaf, below: None, token: Some((row, token)) }) => {
                assert_eq!((row as usize, token), (last, own.generated[last]), "re-aimed at the first id that parts");
                assert_eq!(Some(leaf), tir.logits_leaf_holding(ctx, row, u64::from(token)));
            }
            ("a row's lane", PalwTirAnnexStepV1::Rows(_)) => {}
            (what, _) => panic!("{what}: another step than the descent's"),
        }
    }
}

/// **The whole walk on testnet-12's own fold** (F6 C(1)–C(3) and the one-move court): the IR class
/// registered by the node's own object, its lifecycle row walked `Candidate → Prefetching →
/// Probation → ActiveLimited → Active` by the chain's registry — the admission jury seated on the
/// node's readiness V2 proofs, the probation passed by IR claims the node produced going `Final` —
/// an honest claim of the `Active` class going `Final`, planted lies convicted in one move (licensed,
/// and refused its licence), and a borrowed root convicted by kind 7 (`TirIdentityMismatch`).
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
        step_anchored(c, daa, objects, work, key, subsidy, Hash64::default())
    }

    /// [`step`], the block's own attempt carried with its header's execution anchor (`job_anchor`,
    /// the processor's `extras.own_job_anchor`): past `palw_offence_attribution` the claim records it
    /// as its job identity, which an identity offence (kind 4, kind 7) is judged against.
    fn step_anchored(
        c: &mut rcore::Chain,
        daa: u64,
        objects: &[PalwConsensusObjectV2],
        work: PalwBlockWorkV3<'_>,
        key: Hash64,
        subsidy: u64,
        job_anchor: Hash64,
    ) {
        use kaspa_consensus_core::palw_state_v2::{PalwStateCarriageV2, apply_delta_v2, revert_delta_v2};
        assert!(daa > c.daa, "DAA moves forward");
        let at = rcore::ctx(0xCA_0000 + daa, daa, daa, subsidy);
        let parent = c.s.clone();
        let (child, delta, skips) =
            rcore::fold_with(&c.p, &c.sp, &parent, &at, objects, work, key, &rcore::tape_extras(c, false, daa, job_anchor))
                .unwrap_or_else(|e| panic!("the block at DAA {daa} folds: {e}"));
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
        ir_claim_of(c, ir, n, seed, daa, lie, None)
    }

    /// [`ir_claim_lying`], the committed execution borrowed — when `borrowed` names an anchor — from
    /// that anchor's job instead of the one the claim's own anchor names (an honest run of the wrong
    /// question: kind 7's J1).
    fn ir_claim_of(
        c: &mut rcore::Chain,
        ir: &Ir,
        n: u64,
        seed: u64,
        daa: u64,
        lie: Option<u64>,
        borrowed: Option<Hash64>,
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
        let (job, prompt) = backend.job_for_anchor(borrowed.unwrap_or(anchor)).expect("the anchor's job");
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
        let execution_anchor = execution_anchor_v3(rcore::h(rcore::NET), rcore::h(pre_pow), ir.class_id, &bond.0, nonce);
        let key = execution_commitment_v3(&env.attempt, execution_anchor);
        let id = attempt_id_v2(&env.attempt);
        let at = rcore::ctx(0xCA_0000 + daa, daa, daa, rcore::T12_BLOCK_SUBSIDY_SOMPI);
        let extras = rcore::tape_extras(c, false, daa, execution_anchor);
        if let Err(e) = rcore::fold_with(&c.p, &c.sp, &c.s.clone(), &at, &[], PalwBlockWorkV3::Attempt(&env), key, &extras) {
            eprintln!("DAA {daa}: the class gate refuses the IR attempt: {e}");
            return None;
        }
        step_anchored(c, daa, &[], PalwBlockWorkV3::Attempt(&env), key, rcore::T12_BLOCK_SUBSIDY_SOMPI, execution_anchor);
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
        // testnet-12 arms `palw_offence_attribution` at genesis: its fold runs with it, as here.
        c.attribution = true;
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
        let (label, mut accusation) = super::super::tir_court::palw_tir_one_move_accusation_to_file_v1(
            case.candidates,
            &target,
            &tir.class().program,
            accuser,
            &court,
            ladder,
            form,
        )
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

        // 8. A lie its seats refuse to license meets the court anyway: the seat whose replay refuted
        //    it accuses it at once from its duty (`palw_tir_duty_target_v1`), before any licence — so
        //    a planted fault is convicted whether or not a panel was fooled.
        let lie = honest_leaves / 3;
        keep_ready(&mut c, &ir, registrant);
        let (id, material, roots) = loop {
            seed += 1;
            let daa = c.daa + 1;
            if let Some(claim) = ir_claim_lying(&mut c, &ir, 1, seed, daa, Some(lie)) {
                break claim;
            }
            empty(&mut c, daa);
        };
        bind(&mut c, id, &seats);
        let seat = seats[1].0;
        let duty = kaspa_consensus_core::palw_producer_v2::palw_seat_duties_v2(&c.s, &c.sp, &[seat])
            .into_iter()
            .find(|d| d.claim_id == id)
            .expect("the seat's duty on the bound lie");
        let (job, prompt) = backend.job_for_anchor(roots.anchor).expect("the anchor's job");
        let job = kaspa_consensus_core::palw_attempt_v2::palw_attempt_job_v1(job, false);
        let replay = backend.execute_for_verdict(&job, &prompt).expect("the seat's replay");
        assert_ne!(replay.execution_root, roots.execution_root, "the seat's replay refutes the claim");
        let own = backend.execute(&job, &prompt).expect("the seat's own run");
        let target = super::super::tir_court::palw_tir_duty_target_v1(&duty);
        let case = super::super::tir_court::palw_tir_one_move_case_v1(&tir, &material, &own.material, &rules)
            .expect("the case builds")
            .expect("the lie is a case");
        assert_eq!(case.leaf, Some(lie));
        let (_, mut accusation) = super::super::tir_court::palw_tir_one_move_accusation_to_file_v1(
            case.candidates,
            &target,
            &tir.class().program,
            seat,
            &court,
            ladder,
            form,
        )
        .expect("a close convicts");
        accusation.signature = vec![6; 16];
        let before = c.s.bond(&producer).expect("the producer").collateral;
        let next = c.daa + 1;
        step(
            &mut c,
            next,
            &[PalwConsensusObjectV2::TirShardCourtAccused { accusation: Box::new(accusation) }],
            PalwBlockWorkV3::None,
            Hash64::default(),
            0,
        );
        assert!(
            matches!(c.claim(&id).phase, PalwClaimPhaseV2::Voided { .. }),
            "the unlicensed lie is voided: {:?}",
            c.claim(&id).phase
        );
        assert!(c.s.bond(&producer).expect("the producer").collateral < before, "the producer is charged again");
        eprintln!("walk: an unlicensed lie at leaf {lie} is convicted in one move by the seat that refuted it");

        // 9. Kind 7: a claim whose committed execution is an honest run of ANOTHER job (a borrowed
        //    root) — the seat's J1 probe finds it (`palw_tir_borrowed_root_binding_v1`), the identity
        //    check the gate will run names the fault (`palw_tir_identity_fault_v1`), and the object the
        //    node files (`palw_tir_identity_object_v1`, the program stripped) folds: the claim is voided
        //    and its producer charged.
        keep_ready(&mut c, &ir, registrant);
        let (id, material, roots) = loop {
            seed += 1;
            let daa = c.daa + 1;
            if let Some(claim) = ir_claim_of(&mut c, &ir, 1, seed, daa, None, Some(rcore::h(0xB0B0))) {
                break claim;
            }
            empty(&mut c, daa);
        };
        bind(&mut c, id, &seats);
        let duty = kaspa_consensus_core::palw_producer_v2::palw_seat_duties_v2(&c.s, &c.sp, &[seats[2].0])
            .into_iter()
            .find(|d| d.claim_id == id)
            .expect("the seat's duty on the borrowed claim");
        let binding = crate::palw_panel::reporter_filer::palw_tir_borrowed_root_binding_v1(backend.as_ref(), &material, roots)
            .expect("the capture reproduces the claim's roots under another job");
        let rules = kaspa_consensus_core::palw_offence_attribution_v1::PalwIdentityRulesV1 {
            prompt_ids_form: form,
            base_class_id: bundle.base_class_id,
            da_signer_liability: false,
        };
        let fault = crate::palw_panel::reporter_filer::palw_tir_identity_fault_v1(&duty, &binding, rules).expect("an identity fault");
        let object = kaspa_consensus_core::palw_offence_attribution_v1::palw_tir_identity_object_v1(duty.executor_bond, id, &binding);
        let before = c.s.bond(&producer).expect("the producer").collateral;
        let next = c.daa + 1;
        step(&mut c, next, &[object], PalwBlockWorkV3::None, Hash64::default(), 0);
        assert!(
            matches!(c.claim(&id).phase, PalwClaimPhaseV2::Voided { .. }),
            "the borrowed claim is voided: {:?}",
            c.claim(&id).phase
        );
        assert!(c.s.bond(&producer).expect("the producer").collateral < before, "its producer is charged");
        eprintln!("walk: a borrowed IR root is convicted by kind 7 ({fault:?})");
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
