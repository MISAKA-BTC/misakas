//! **G14 lane D — a model registration, on the REAL consensus path, against the offline gate.**
//!
//! The offline tooling (the SDK's preflight, `getPalwModelPreflight`, `palw-class`) answers "would the
//! chain take this class" with ONE function, `palw_tir_registration_preflight_at_v1` — the IR fence,
//! then admission v10 at a height. The chain answers with the processor's acceptance walk
//! (`palw_v2_accepted_objects`): drop-by-name below the fence, the bond's signature, the chain's target,
//! admission v10 under the rules the PROCESSOR resolves, the share rule, then the fold's own refusals
//! (duplicate, activation lookahead, exposure, burn). This file holds the two to one table.
//!
//! **What "parity" means here** (the gate's own doc says which checks it does not hold: "the registrant
//! bond's signature and collateral, the chain's target, and the share the certification decides"):
//!
//! * the gate REFUSES  => the chain refuses, and the chain's sentence carries the gate's own error text;
//! * the chain ACCEPTS => the gate admits;
//! * the gate admits and the chain drops => only for a STATEFUL reason the gate cannot see, and named.
//!
//! Every case runs on the real testnet-12 harness (`t12_with_harness_cards`: the launched ruleset, the
//! eight genesis cards on harness keys, `palw_tir_v1` armed), through the node's own acceptance walk, and
//! the pipeline cases (`g14_*_mined_*`) through the mempool, the node's own block template, the
//! chain block's fold, the persisted tip, the ConsensusApi reads, a second node replaying the blocks, and
//! a reorg.
use super::t12_round_lane_e2e::{
    T12Chain, card_payout_spk, sign_spend, t12_genesis_chain, t12_genesis_chain_on, t12_reopened_chain, t12_with_harness_cards,
};
use crate::consensus::test_consensus::TestConsensus;
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::config::params::ForkActivation;
use kaspa_consensus_core::config::{Config, ConfigBuilder};
use kaspa_consensus_core::palw_artifact::{artifact_leaf_v1, artifact_root_v1};
use kaspa_consensus_core::palw_class_admission_v2::PalwClassAdmissionError as E;
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwCertifiedLaneV1, PalwChainStateV2, PalwConsensusObjectV2 as Obj, PalwStateV2Error,
};
use kaspa_consensus_core::palw_step_refute::tiled_logits_scheme_id_v1;
use kaspa_consensus_core::palw_tir_admission_v1::{palw_tir_post_genesis_registration_v1, palw_tir_registration_preflight_at_v1};
use kaspa_consensus_core::palw_tir_artifact_v1::{PalwTirTensorSourceV1, palw_tir_inventory_operands_v1};
use kaspa_consensus_core::palw_tir_attempt_v1::{PalwTirJobFactsV1, palw_tir_attempt_canonical_v1, palw_tir_job_context_v1};
use kaspa_consensus_core::palw_tir_class_v1::{
    PALW_TIR_CLASS_REGISTRATION_MLDSA87_CONTEXT_V1, PALW_TIR_CLASS_VERSION_V1, PALW_TIR_LAYOUT_VERSION_V1, PalwTirClassV1,
    PalwTirLayoutV1, palw_tir_class_registration_message_v1,
};
use kaspa_consensus_core::palw_tir_v1::PalwTirFenceV1;
use kaspa_consensus_core::tx::{Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry};
use kaspa_hashes::Hash64;
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::path::PathBuf;

type Premine = Vec<(TransactionOutpoint, UtxoEntry)>;

// ---- fixtures: the corpus programs as classes, with their real inventory roots -------------------------

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex")).collect()
}

struct Tensors(BTreeMap<(u16, Option<u16>), Vec<u8>>);
impl PalwTirTensorSourceV1 for Tensors {
    fn tensor_bytes(&self, param: u16, layer: Option<u16>) -> Option<Cow<'_, [u8]>> {
        self.0.get(&(param, layer)).map(|b| Cow::Borrowed(b.as_slice()))
    }
}

/// The layout the court fixture declares (ragged commit tiles, a 4,096-lane logits tile, two-position
/// checkpoints) for `class`'s program at `positions` — the layout `declare-layout` would write.
fn layout_of(class: &PalwTirClassV1, positions: u32) -> PalwTirLayoutV1 {
    let p = class.decode_program().expect("canonical");
    let mut tiles = Vec::new();
    let mut k = 0u32;
    for (bi, b) in p.blocks.iter().enumerate() {
        for (ni, n) in b.nodes.iter().enumerate() {
            if !n.commit {
                continue;
            }
            let is_logits = bi == p.schedule.post as usize && ni == p.logits as usize;
            tiles.push(if is_logits { 4096 } else { 4 + (k * 7) % 6 });
            k += 1;
        }
    }
    PalwTirLayoutV1 {
        version: PALW_TIR_LAYOUT_VERSION_V1,
        max_context: positions,
        checkpoint_interval: 2,
        h_tile: 2,
        commit_tiles: tiles,
        state_tiles: (0..p.states.len() as u32).map(|j| 4 + j % 3).collect(),
    }
}

#[derive(Clone)]
struct Case {
    name: String,
    class: PalwTirClassV1,
    root: Hash64,
    /// The inventory root is the real one over the corpus tensors (`true`) or absent (a program with no params).
    real_root: bool,
}

/// Every golden corpus program as a class at 64 positions, under the tiled logits scheme, with the
/// inventory root its tensors derive.
fn corpus() -> Vec<Case> {
    corpus_at(64)
}

/// [`corpus`] declared at `positions` of context.
fn corpus_at(positions: u32) -> Vec<Case> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../consensus-vectors/tir-v1/programs");
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("vectors")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();
    files
        .into_iter()
        .map(|path| {
            let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            let mut class = PalwTirClassV1 {
                version: PALW_TIR_CLASS_VERSION_V1,
                program: unhex(v["program_borsh_hex"].as_str().unwrap()),
                layout: PalwTirLayoutV1 {
                    version: PALW_TIR_LAYOUT_VERSION_V1,
                    max_context: positions,
                    checkpoint_interval: 2,
                    h_tile: 2,
                    commit_tiles: Vec::new(),
                    state_tiles: Vec::new(),
                },
                tokenizer_id: Hash64::from_bytes([0x70; 64]),
            };
            let mut program = class.decode_program().expect("canonical");
            program.logits_scheme_id.copy_from_slice(tiled_logits_scheme_id_v1().as_byte_slice());
            class.program = program.encode();
            class.layout = layout_of(&class, positions);
            let mut tensors = BTreeMap::new();
            for p in v["params"].as_array().unwrap() {
                let j = p["param"].as_u64().unwrap() as u16;
                let layer = p["layer"].as_u64().map(|l| l as u16);
                tensors.insert((j, layer), unhex(p["le_hex"].as_str().unwrap()));
            }
            let (root, real_root) = if tensors.is_empty() {
                (Hash64::from_bytes([0; 64]), false)
            } else {
                let ops = palw_tir_inventory_operands_v1(&program, &Tensors(tensors)).expect("the inventory");
                (artifact_root_v1(&ops.iter().map(artifact_leaf_v1).collect::<Vec<_>>()).expect("a root"), true)
            };
            Case { name: v["name"].as_str().unwrap().to_string(), class, root, real_root }
        })
        .collect()
}

fn case(name: &str) -> Case {
    corpus().into_iter().find(|c| c.name == name).unwrap_or_else(|| panic!("the corpus has {name}"))
}

// ---- the environment: testnet-12's harness chain with the IR fence armed ------------------------------------

struct Env {
    chain: T12Chain,
    config: Config,
    bundle: PalwConsensusParamsV2,
    premine: Premine,
    floats: Premine,
    domain: Hash64,
}

/// testnet-12 as launched, harness cards, `palw_tir_v1` armed at `tir_at`.
fn t12_tir_config(tir_at: u64) -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    let (config, _, premine, floats) = t12_with_harness_cards();
    let mut params = config.params.clone();
    params.palw_tir_v1 = Some(PalwTirFenceV1::testnet12_v1(ForkActivation::new(tir_at)));
    params.sync_palw_tir_v1();
    let armed = ConfigBuilder::new(params).skip_proof_of_work().build();
    armed.params.validate_palw_v2().expect("testnet-12 with the IR armed is a runnable ruleset");
    let PalwConsensusMode::ConsensusV2(bundle) = &armed.params.palw_consensus_mode else { unreachable!() };
    let bundle = bundle.clone();
    (armed, bundle, premine, floats)
}

impl Env {
    fn new(tir_at: u64) -> Env {
        Env::over(t12_tir_config(tir_at))
    }

    fn over(parts: (Config, PalwConsensusParamsV2, Premine, Premine)) -> Env {
        let chain = t12_genesis_chain(&parts.0, &parts.1, &parts.2, &parts.3);
        Env::around(chain, parts)
    }

    fn around(chain: T12Chain, (config, bundle, premine, floats): (Config, PalwConsensusParamsV2, Premine, Premine)) -> Env {
        let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
            config.params.net.to_string().as_bytes(),
            Some(config.params.genesis.hash),
        );
        Env { chain, config, bundle, premine, floats, domain }
    }

    /// The chain's own pricing, as `getPalwRegistrationTerms` serves it: (initial target, slash value, ladder).
    fn terms(&self, state: &PalwChainStateV2) -> (u128, u64, u64) {
        let floor = self.bundle.base_class_id;
        (
            state.class_target(&floor).expect("the floor's target").target,
            state.class(&floor).expect("the floor class").slash_value_per_pwu,
            self.bundle.court.max_step_leaf_count(),
        )
    }

    fn point(&self, block: BlockHash, daa: u64) -> PalwBlockContextV2 {
        PalwBlockContextV2 { block, daa_score: daa, blue_score: 1, subsidy: 0 }
    }
}

/// What a registration is made of before any mutation.
#[derive(Clone)]
struct Spec {
    class: PalwTirClassV1,
    root: Hash64,
    card: usize,
    share: u16,
    activation_daa: u64,
}

fn spec_of(c: &Case, card: usize, activation_daa: u64) -> Spec {
    Spec { class: c.class.clone(), root: c.root, card, share: 0, activation_daa }
}

/// The object a registrant builds (the SDK's `build_tir_registration_v1`: the canonical job of the formula, the pwu
/// counted against the network's ladder, the chain's own target and slash value), UNSIGNED.
fn build(env: &Env, state: &PalwChainStateV2, s: &Spec) -> Obj {
    let (target, slash, ladder) = env.terms(state);
    let facts = PalwTirJobFactsV1::of_class(&s.class, s.class.class_id(&s.root)).expect("decodes");
    let canonical = palw_tir_job_context_v1(&facts, palw_tir_attempt_canonical_v1(&s.class).expect("wide enough"));
    palw_tir_post_genesis_registration_v1(
        s.class.clone(),
        canonical,
        s.root,
        s.share,
        target,
        slash,
        s.activation_daa,
        env.chain.bonds[s.card],
        Vec::new(),
        ladder,
    )
    .expect("the builder counts the canonical job")
}

/// Sign `object` as card `card`'s bond, over the message of network `domain`.
fn sign(object: &mut Obj, card: usize, domain: Hash64) {
    let Obj::ClassRegisteredTirV1 {
        class_id,
        share_permille,
        activation_daa,
        artifact_root,
        slash_value_per_pwu,
        initial_target,
        pwu_rule,
        admission,
    } = object
    else {
        unreachable!()
    };
    let message = palw_tir_class_registration_message_v1(
        domain,
        *class_id,
        *share_permille,
        *activation_daa,
        &admission.registrant_bond,
        *artifact_root,
        *slash_value_per_pwu,
        *initial_target,
        pwu_rule,
        &admission.canonical,
        &admission.class,
    );
    let key = TestConsensus::palw_v2_registry_keypair(card as u64);
    admission.signature = libcrux_ml_dsa::ml_dsa_87::sign(
        &key.signing_key,
        message.as_byte_slice(),
        PALW_TIR_CLASS_REGISTRATION_MLDSA87_CONTEXT_V1,
        [0x61u8; 32],
    )
    .expect("ML-DSA-87 signs")
    .as_ref()
    .to_vec();
}

fn signed(env: &Env, state: &PalwChainStateV2, s: &Spec) -> Obj {
    let mut o = build(env, state, s);
    sign(&mut o, s.card, env.domain);
    o
}

/// The id the registration declares.
fn class_id_of(o: &Obj) -> Hash64 {
    match o {
        Obj::ClassRegisteredTirV1 { class_id, .. } => *class_id,
        _ => unreachable!(),
    }
}

/// `edit` applied to the object's carried class, then the id and the canonical job's profile id re-derived (so the id
/// check passes and the mutation is judged on its own property), then signed.
fn mutate_class(env: &Env, state: &PalwChainStateV2, s: &Spec, edit: impl FnOnce(&mut PalwTirClassV1)) -> Obj {
    let mut o = build(env, state, s);
    if let Obj::ClassRegisteredTirV1 { class_id, artifact_root, admission, .. } = &mut o {
        edit(&mut admission.class);
        *class_id = admission.class.class_id(artifact_root);
        admission.canonical.shape_profile_id = *class_id;
    }
    sign(&mut o, s.card, env.domain);
    o
}

// ---- one object, judged four ways ------------------------------------------------------------------------

#[derive(Debug)]
struct Judged {
    /// The offline gate: `palw_tir_registration_preflight_at_v1` at the same height, with the chain's certified families.
    preflight: Result<(), E>,
    /// The processor's acceptance arm (`palw_v2_validate_objects`), its sentence.
    validate: Result<(), String>,
    /// The fold on the same state, if the arm took it.
    fold: Option<Result<(), PalwStateV2Error>>,
    /// Whether the node's acceptance walk (the one a block runs) keeps the object.
    accepted: bool,
}

fn judge(env: &Env, state: &PalwChainStateV2, block: BlockHash, daa: u64, o: &Obj) -> Judged {
    let vp = env.chain.vp();
    let point = PalwBlockContextV2 { blue_score: state.last_point().map(|p| p.blue_score + 1).unwrap_or(1), ..env.point(block, daa) };
    let certified = state.chain_certified_families(PalwCertifiedLaneV1::Attempt);
    let preflight = palw_tir_registration_preflight_at_v1(&env.config.params, &env.bundle, o, daa, &certified).map(|_| ());
    let validate = vp.palw_v2_validate_objects(state, &env.bundle.state, &point, std::slice::from_ref(o));
    let fold = validate
        .is_ok()
        .then(|| vp.palw_v2_fold_accepted_for_tests(state, &env.bundle.state, &point, std::slice::from_ref(o)).map(|_| ()));
    let accepted = !vp.palw_v2_accepted_objects_for_tests(state, &env.bundle.state, &point, vec![o.clone()], block).is_empty();
    Judged { preflight, validate, fold, accepted }
}

/// What a case expects of the pair.
#[derive(Debug, Clone, Copy)]
enum Want {
    /// Gate admits, chain keeps it.
    Accept,
    /// Gate refuses with this code; the chain refuses at its admission arm and its sentence carries the gate's text.
    Refuse(&'static str),
    /// Gate admits; the chain drops it for a reason the gate does not hold, whose sentence has this text (the arm's
    /// or, past the arm, the fold's).
    StatefulDrop(&'static str),
}

fn check(name: &str, j: &Judged, want: Want) -> Result<(), String> {
    let bad = |why: String| Err(format!("{name}: {why} ({j:?})"));
    match want {
        Want::Accept => {
            if j.preflight != Ok(()) {
                return bad("the gate should admit".into());
            }
            if j.validate != Ok(()) || !matches!(j.fold, Some(Ok(()))) || !j.accepted {
                return bad("the chain should keep it".into());
            }
        }
        Want::Refuse(code) => {
            let Err(e) = j.preflight.as_ref() else { return bad("the gate should refuse".into()) };
            if e.code() != code {
                return bad(format!("the gate's code should be {code}, it is {} ({e})", e.code()));
            }
            let Err(why) = j.validate.as_ref() else { return bad("the arm should refuse".into()) };
            if !why.contains(&e.to_string()) {
                return bad(format!("the chain's sentence `{why}` should carry the gate's `{e}`"));
            }
            if j.fold.is_some() || j.accepted {
                return bad("nothing should be folded".into());
            }
        }
        Want::StatefulDrop(text) => {
            if j.preflight != Ok(()) {
                return bad("the gate should admit what it cannot see to refuse".into());
            }
            let why = match (&j.validate, &j.fold) {
                (Err(why), _) => why.clone(),
                (Ok(()), Some(Err(e))) => e.to_string(),
                _ => return bad("the chain should drop it".into()),
            };
            if !why.contains(text) {
                return bad(format!("the chain's reason `{why}` should name `{text}`"));
            }
            if j.accepted {
                return bad("the acceptance walk should drop it".into());
            }
        }
    }
    Ok(())
}

/// Run every case, print one line each, and fail with ALL the disagreements.
fn run_table(env: &Env, state: &PalwChainStateV2, block: BlockHash, daa: u64, cases: Vec<(&str, Obj, Want)>) {
    let mut bad = Vec::new();
    for (name, o, want) in cases {
        let j = judge(env, state, block, daa, &o);
        let gate = match &j.preflight {
            Ok(()) => "admits".to_string(),
            Err(e) => format!("refuses {}", e.code()),
        };
        let chain = match (&j.validate, &j.fold, j.accepted) {
            (Ok(()), Some(Ok(())), true) => "keeps".to_string(),
            (Err(why), _, _) => format!("refuses ({})", why.chars().take(90).collect::<String>()),
            (Ok(()), Some(Err(e)), _) => format!("drops in the fold ({})", e.to_string().chars().take(90).collect::<String>()),
            other => format!("{other:?}"),
        };
        eprintln!("[g14] {name:<52} gate {gate:<40} | chain {chain}");
        if let Err(e) = check(name, &j, want) {
            bad.push(e);
        }
    }
    assert!(bad.is_empty(), "preflight/consensus disagreements:\n{}", bad.join("\n"));
}

// ---- the table -------------------------------------------------------------------------------------------

/// The five corpus models the range analysis proves are admitted by the chain and the gate alike; the two it
/// cannot prove (a state saturating its fixed width, a history window) are refused by the program's own gate by both.
#[tokio::test]
async fn g14_parity_corpus_classes_real_inventory_roots() {
    let env = Env::new(0);
    let (block, state) = env.chain.tip_state();
    let daa = env.chain.daa_of(block);
    let mut accepted = Vec::new();
    for (i, c) in corpus().iter().enumerate() {
        let o = signed(&env, &state, &spec_of(c, 1 + i % 6, daa));
        let j = judge(&env, &state, block, daa, &o);
        match &j.preflight {
            Ok(()) => {
                check(&c.name, &j, Want::Accept).unwrap();
                accepted.push(c.name.clone());
            }
            Err(E::TirProgram(_)) => check(&c.name, &j, Want::Refuse("TIR_PROGRAM_REFUSED")).unwrap(),
            Err(e) => panic!("{}: unexpected refusal {e}", c.name),
        }
        eprintln!("[g14] corpus {:>24}: real_root={} gate={:?} chain_accepted={}", c.name, c.real_root, j.preflight, j.accepted);
    }
    assert_eq!(accepted.len(), 5, "the five corpus models are admitted by both: {accepted:?}");
}

/// `edit` applied to the whole object (any field, the carried class included) BEFORE the signature, with nothing
/// re-derived: the mutation is judged on its own property and the signature is the registrant's over exactly these bytes.
fn edit_raw(env: &Env, state: &PalwChainStateV2, s: &Spec, edit: impl FnOnce(&mut Obj)) -> Obj {
    let mut o = build(env, state, s);
    edit(&mut o);
    sign(&mut o, s.card, env.domain);
    o
}

macro_rules! reg {
    ($o:expr, { $($field:ident),* $(,)? } => $body:expr) => {
        if let Obj::ClassRegisteredTirV1 { $($field,)* .. } = $o {
            $body
        }
    };
}

/// The object of the EDITED class, root and pricing consistent (its canonical job, its pwu, its signature), carrying the
/// ORIGINAL class id: a registration whose only fault is that its id is not the derived one — an id lifted onto
/// another tokenizer, another weights root, another layout.
fn stale_id(env: &Env, state: &PalwChainStateV2, s: &Spec, edit: impl FnOnce(&mut PalwTirClassV1, &mut Hash64)) -> Obj {
    let old = s.class.class_id(&s.root);
    let mut edited = s.clone();
    edit(&mut edited.class, &mut edited.root);
    let mut o = build(env, state, &edited);
    reg!(&mut o, { class_id } => *class_id = old);
    sign(&mut o, s.card, env.domain);
    o
}

fn network_domain(name: &str, genesis: Hash64) -> Hash64 {
    kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(name.as_bytes(), Some(genesis))
}

#[tokio::test]
async fn g14_adversarial_registrations_gate_and_chain_agree() {
    use kaspa_consensus_core::palw_state_v2::PalwPwuRuleV2;
    let env = Env::new(0);
    let (block, state) = env.chain.tip_state();
    let daa = env.chain.daa_of(block);
    let dense = case("dense-gqa-2layer");
    let moe = case("moe-top2-shared");
    let mamba = case("mamba2");
    let s = spec_of(&dense, 1, daa);
    let counted = match build(&env, &state, &s) {
        Obj::ClassRegisteredTirV1 { pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference }, .. } => pwu_per_inference,
        _ => unreachable!(),
    };
    let (target, slash, _) = env.terms(&state);
    let genesis = env.config.params.genesis.hash;
    let vocab = {
        let p = dense.class.decode_program().unwrap();
        p.blocks[p.schedule.post as usize].nodes[p.logits as usize].out.elements_at(1) as u32
    };
    let unknown_op_tag = {
        // The first byte whose replacement by 0xFE is an unknown Borsh tag: an operation no primitive set names.
        let bytes = &dense.class.program;
        (0..bytes.len())
            .find(|i| {
                let mut m = bytes.clone();
                m[*i] = 0xFE;
                kaspa_consensus_core::palw_tir_class_v1::PalwTirClassV1 { program: m, ..dense.class.clone() }
                    .decode_program()
                    .err()
                    .is_some_and(|e| e.to_string().to_lowercase().contains("variant") || e.to_string().to_lowercase().contains("tag"))
            })
            .expect("some byte is an operation tag")
    };
    let cases: Vec<(&str, Obj, Want)> = vec![
        ("baseline: dense-gqa-2layer, signed, the chain's terms", signed(&env, &state, &s), Want::Accept),
        // ---- the program ----
        (
            "program: a trailing byte (not the canonical encoding)",
            mutate_class(&env, &state, &s, |c| c.program.push(0)),
            Want::Refuse("TIR_PROGRAM_REFUSED"),
        ),
        (
            "program: an unknown operation tag (unsupported op masquerading as a known one)",
            mutate_class(&env, &state, &s, |c| c.program[unknown_op_tag] = 0xFE),
            Want::Refuse("TIR_PROGRAM_REFUSED"),
        ),
        (
            "program: another primitive set's id",
            mutate_class(&env, &state, &s, |c| {
                let mut p = c.decode_program().unwrap();
                p.prim_set_id[0] ^= 1;
                c.program = p.encode();
            }),
            Want::Refuse("TIR_PROGRAM_REFUSED"),
        ),
        (
            "program: an unknown logits scheme",
            mutate_class(&env, &state, &s, |c| {
                let mut p = c.decode_program().unwrap();
                p.logits_scheme_id = [0; 64];
                c.program = p.encode();
            }),
            Want::Refuse("TIR_CLASS_REFUSED"),
        ),
        (
            "program: token bound below the vocabulary",
            mutate_class(&env, &state, &s, |c| {
                let mut p = c.decode_program().unwrap();
                p.token_bound = vocab - 1;
                c.program = p.encode();
            }),
            Want::Refuse("TIR_CLASS_REFUSED"),
        ),
        (
            "program: the held history bound (2^21)",
            mutate_class(&env, &state, &s, |c| {
                let mut p = c.decode_program().unwrap();
                p.history_bound = 1 << 21;
                c.program = p.encode();
            }),
            Want::Refuse("TIR_CLASS_REFUSED"),
        ),
        // ---- the tokenizer ----
        (
            "tokenizer: swapped, the id left as signed for the old one",
            stale_id(&env, &state, &s, |c, _| c.tokenizer_id = Hash64::from_bytes([0x99; 64])),
            Want::Refuse("TIR_CLASS_ID_IS_NOT_DERIVED"),
        ),
        (
            "tokenizer: swapped, the id re-derived (a different class: inert)",
            mutate_class(&env, &state, &s, |c| c.tokenizer_id = Hash64::from_bytes([0x99; 64])),
            Want::Accept,
        ),
        // ---- the artifact ----
        (
            "artifact: root swapped, the id left for the old root",
            stale_id(&env, &state, &s, |_, root| *root = Hash64::from_bytes([0x13; 64])),
            Want::Refuse("TIR_CLASS_ID_IS_NOT_DERIVED"),
        ),
        (
            "artifact: root swapped AND the id re-derived (a different class: inert)",
            edit_raw(&env, &state, &s, |o| {
                reg!(o, { class_id, artifact_root, admission } => {
                    *artifact_root = Hash64::from_bytes([0x13; 64]);
                    *class_id = admission.class.class_id(artifact_root);
                    admission.canonical.shape_profile_id = *class_id;
                })
            }),
            Want::Accept,
        ),
        // ---- the layout ----
        (
            "layout: another program's (mamba2's commit tiles on dense)",
            mutate_class(&env, &state, &s, |c| c.layout = layout_of(&mamba.class, 64)),
            Want::Refuse("TIR_CLASS_REFUSED"),
        ),
        (
            "layout: max_context changed, the id left for 64",
            stale_id(&env, &state, &s, |c, _| c.layout.max_context = 48),
            Want::Refuse("TIR_CLASS_ID_IS_NOT_DERIVED"),
        ),
        (
            "layout: a commit tile dropped",
            mutate_class(&env, &state, &s, |c| {
                c.layout.commit_tiles.pop();
            }),
            Want::Refuse("TIR_CLASS_REFUSED"),
        ),
        // ---- the plan / canonical job / pwu ----
        (
            "plan: the canonical job one decode token longer",
            edit_raw(&env, &state, &s, |o| reg!(o, { admission } => admission.canonical.exact_decode_tokens += 1)),
            Want::Refuse("CLASS_NOT_ATTRIBUTABLE"),
        ),
        (
            "plan: pwu rule MaxPerAttempt on an IR class",
            edit_raw(&env, &state, &s, |o| reg!(o, { pwu_rule } => *pwu_rule = PalwPwuRuleV2::MaxPerAttempt(5))),
            Want::Refuse("CLASS_IS_NOT_DERIVED"),
        ),
        (
            "plan: pwu_per_inference one more than counted",
            edit_raw(
                &env,
                &state,
                &s,
                |o| reg!(o, { pwu_rule } => *pwu_rule = PalwPwuRuleV2::DerivedV1 { pwu_per_inference: counted + 1 }),
            ),
            Want::Refuse("PWU_PER_INFERENCE_MISMATCH"),
        ),
        // ---- the family / share claim ----
        (
            "family claim: share_permille 1 for a class no certified family covers",
            edit_raw(&env, &state, &s, |o| reg!(o, { share_permille } => *share_permille = 1)),
            Want::Refuse("NOT_END_TO_END_CERTIFIED"),
        ),
        // ---- what the gate does not hold: chain state ----
        (
            "target: not the chain's (difficulty is not a registrant's to choose)",
            edit_raw(&env, &state, &s, |o| reg!(o, { initial_target } => *initial_target = target + 1)),
            Want::StatefulDrop("difficulty is not a registrant's to choose"),
        ),
        (
            "slash value: not the network's",
            edit_raw(&env, &state, &s, |o| reg!(o, { slash_value_per_pwu } => *slash_value_per_pwu = slash + 1)),
            Want::StatefulDrop("this network's unit is"),
        ),
        (
            "slash value: zero",
            edit_raw(&env, &state, &s, |o| reg!(o, { slash_value_per_pwu } => *slash_value_per_pwu = 0)),
            Want::StatefulDrop("zero slash value"),
        ),
        (
            "activation: 4,001 DAA ahead (the lookahead is 4,000)",
            edit_raw(&env, &state, &s, |o| reg!(o, { activation_daa } => *activation_daa = daa + 4_001)),
            Want::StatefulDrop("schedules its activation"),
        ),
        (
            "signature: another card's key over the registrant's bond",
            {
                let mut o = build(&env, &state, &s);
                sign(&mut o, 3, env.domain);
                o
            },
            Want::StatefulDrop("is not signed by the bond it names"),
        ),
        (
            "signature: replayed from testnet-11 (another network id)",
            {
                let mut o = build(&env, &state, &s);
                sign(&mut o, s.card, network_domain("kaspa-testnet-11", genesis));
                o
            },
            Want::StatefulDrop("is not signed by the bond it names"),
        ),
        (
            "signature: replayed from the same network name on another genesis",
            {
                let mut o = build(&env, &state, &s);
                sign(&mut o, s.card, network_domain(&env.config.params.net.to_string(), Hash64::from_bytes([0xAB; 64])));
                o
            },
            Want::StatefulDrop("is not signed by the bond it names"),
        ),
        ("signature: none (unsigned)", build(&env, &state, &s), Want::StatefulDrop("is not signed by the bond it names")),
        (
            "registrant: a bond this chain does not have",
            edit_raw(&env, &state, &s, |o| {
                reg!(o, { admission } => {
                    admission.registrant_bond = kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(TransactionOutpoint::new(
                        kaspa_consensus_core::tx::TransactionId::from_bytes([0x7E; 64]),
                        0,
                    ))
                })
            }),
            Want::StatefulDrop("bond this chain does not have"),
        ),
    ];
    let _ = (&moe, &moe.name);
    run_table(&env, &state, block, daa, cases);
}

/// **The same registration at different heights**: the gate's `daa` and the chain's block DAA are one number, so the fence
/// is crossed by both at the same height — below it the gate names the fence and the walk drops the object by name.
#[tokio::test]
async fn g14_fence_height_is_the_same_for_gate_and_chain() {
    const FENCE: u64 = 5;
    let env = Env::new(FENCE);
    let (block, state) = env.chain.tip_state();
    let dense = case("dense-gqa-2layer");
    let o = signed(&env, &state, &spec_of(&dense, 2, 0));
    for daa in [0, 1, FENCE - 1] {
        let j = judge(&env, &state, block, daa, &o);
        assert_eq!(j.preflight, Err(E::TirNeedsItsFence), "daa {daa}: the gate names the fence ({j:?})");
        let why = j.validate.as_ref().expect_err("the arm refuses");
        assert!(why.contains("palw_tir_v1 is not in force"), "daa {daa}: {why}");
        assert!(!j.accepted, "daa {daa}: the walk drops it by name");
    }
    for daa in [FENCE, FENCE + 1, 1_000, 100_000] {
        check(&format!("daa {daa}"), &judge(&env, &state, block, daa, &o), Want::Accept).unwrap();
    }
}

#[tokio::test]
async fn g14_duplicate_registrations() {
    let env = Env::new(0);
    let (block, state) = env.chain.tip_state();
    let dense = case("dense-gqa-2layer");
    let o = signed(&env, &state, &spec_of(&dense, 2, 0));
    let vp = env.chain.vp();
    let point = env.point(block, 0);
    // The same object twice in one block: the walk keeps the first only (the IR registration cap, then the fold's second lock).
    let kept = vp.palw_v2_accepted_objects_for_tests(&state, &env.bundle.state, &point, vec![o.clone(), o.clone()], block);
    assert_eq!(kept, vec![o.clone()], "one of two identical registrations in a block");
    // In a later block: the class exists, the same object (and the same class from another registrant) is a duplicate.
    let after = vp.palw_v2_fold_accepted_for_tests(&state, &env.bundle.state, &point, std::slice::from_ref(&o)).expect("folds");
    assert!(after.class(&class_id_of(&o)).is_some());
    let again = judge(&env, &after, block, 0, &o);
    check("duplicate: the same object again", &again, Want::StatefulDrop("already")).unwrap();
    let by_another = signed(&env, &after, &spec_of(&dense, 3, 0));
    assert_eq!(class_id_of(&by_another), class_id_of(&o), "one class id");
    check(
        "duplicate: the same class by another registrant",
        &judge(&env, &after, block, 0, &by_another),
        Want::StatefulDrop("already"),
    )
    .unwrap();
}

/// **A differential over mutated programs**: every sampled single-byte mutation of the dense and the MoE corpus
/// programs (the whole header, then a stride through the body), carried as a registration whose id, canonical job and
/// signature are consistent with the mutated class, is judged by the gate and by the chain. The two must agree on
/// admit/refuse, and a refusal must carry the gate's own text — so no mutation is a class the chain keeps and the gate
/// refuses, or the reverse.
#[tokio::test]
async fn g14_program_mutation_differential() {
    let env = Env::new(0);
    let (block, state) = env.chain.tip_state();
    let daa = env.chain.daa_of(block);
    let mut stats: BTreeMap<String, (u32, u32)> = BTreeMap::new();
    let mut bad = Vec::new();
    for name in ["dense-gqa-2layer", "moe-top2-shared"] {
        let c = case(name);
        let base = spec_of(&c, 1, daa);
        let len = c.class.program.len();
        let positions: Vec<usize> = (0..len.min(96)).chain((96..len).step_by(41)).collect();
        for (n, i) in positions.into_iter().enumerate() {
            for xor in [0x01u8, 0x80] {
                let o = mutate_class(&env, &state, &base, |class| class.program[i] ^= xor);
                let vp = env.chain.vp();
                let point = env.point(block, daa);
                let certified = state.chain_certified_families(PalwCertifiedLaneV1::Attempt);
                let gate = palw_tir_registration_preflight_at_v1(&env.config.params, &env.bundle, &o, daa, &certified).map(|_| ());
                let arm = vp.palw_v2_validate_objects(&state, &env.bundle.state, &point, std::slice::from_ref(&o));
                let key = match &gate {
                    Ok(()) => "admitted".to_string(),
                    Err(e) => e.code().to_string(),
                };
                let entry = stats.entry(key).or_default();
                match (&gate, &arm) {
                    (Ok(()), Ok(())) => entry.0 += 1,
                    (Err(e), Err(why)) if why.contains(&e.to_string()) => entry.1 += 1,
                    other => bad.push(format!("{name} byte {i} ^ {xor:#x} (#{n}): {other:?}")),
                }
            }
        }
    }
    eprintln!("[g14] mutation differential (code -> (admitted by both, refused by both)): {stats:?}");
    assert!(bad.is_empty(), "the gate and the chain disagree:\n{}", bad.join("\n"));
    assert!(stats.len() >= 3, "the mutations reach several refusal classes: {stats:?}");
}

// ---- the pipeline: carrier -> mempool -> template -> block -> fold -> persisted tip -> reads ---------------

/// A 0x4b lifecycle carrier for `object`, spending `funding` (owned by card `card`'s payout key) and signed by that card.
fn carrier_from(env: &Env, object: &Obj, card: usize, funding: (TransactionOutpoint, UtxoEntry), fee: u64) -> Transaction {
    use kaspa_consensus_core::palw_lifecycle_objects_v2::{PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2};
    let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object: object.clone() })
        .expect("serializes");
    let (outpoint, entry) = funding;
    let mut tx = Transaction::new(
        crate::constants::TX_VERSION,
        vec![TransactionInput::new(outpoint, vec![], 0, 1)],
        vec![TransactionOutput::new(entry.amount - fee, card_payout_spk(card))],
        0,
        kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
        0,
        payload,
    );
    sign_spend(&mut tx, entry, card, env.config.params.storage_mass_parameter);
    tx
}

/// [`carrier_from`] funded by the card's genesis fee float.
fn carrier_of(env: &Env, object: &Obj, card: usize, fee: u64) -> Transaction {
    carrier_from(env, object, card, env.floats[card].clone(), fee)
}

/// The funding a carrier's change offers the next one.
fn change_of(tx: &Transaction, card: usize) -> (TransactionOutpoint, UtxoEntry) {
    (TransactionOutpoint::new(tx.id(), 0), UtxoEntry::new(tx.outputs[0].value, card_payout_spk(card), 0, false))
}

const CARRIER_FEE: u64 = 2_000_000;

fn mempool_verdict(env: &Env, tx: &Transaction) -> Result<(), kaspa_consensus_core::errors::tx::TxRuleError> {
    env.chain
        .vp()
        .validate_mempool_transaction(&mut kaspa_consensus_core::tx::MutableTransaction::from_tx(tx.clone()), &Default::default())
}

/// One IR registration, mined by the node's own template and folded by the next chain block.
struct Mined {
    env: Env,
    object: Obj,
    class_id: Hash64,
    card: usize,
    carrier: Transaction,
    /// The blocks, oldest first: the warm-up heartbeat, the carrying block, the folding block.
    blocks: Vec<kaspa_consensus_core::block::Block>,
    /// The gate's record (what the offline tooling says the chain will write).
    record: kaspa_consensus_core::palw_tir_admission_v1::PalwTirClassRecordV1,
    activation: u64,
    /// The registrant bond's collateral before the registration.
    collateral_before: u64,
}

const ACTIVATION_AHEAD: u64 = 30;

async fn mine_registration(c: &Case, card: usize) -> Mined {
    mine_registration_on(Env::new(0), c, card).await
}

async fn mine_registration_on(mut env: Env, c: &Case, card: usize) -> Mined {
    let ttpb = env.config.params.target_time_per_block();
    let h1 = env.chain.heartbeat(ttpb, Vec::new()).await;
    let (block, state) = env.chain.tip_state();
    let daa = env.chain.daa_of(block);
    let spec = spec_of(c, card, daa + ACTIVATION_AHEAD);
    let object = signed(&env, &state, &spec);
    let class_id = class_id_of(&object);
    // The terms a registrant builds from (`getPalwRegistrationTerms`) are the chain's own pricing — the numbers `build` used —
    // and do not yet list the class.
    {
        let terms = env.chain.ctx.consensus.palw_v2_registration_terms().expect("the chain serves registration terms");
        let (target, slash, _) = env.terms(&state);
        assert_eq!(
            (terms.initial_target, terms.slash_value_per_pwu),
            (target, slash),
            "the served terms are the pricing the object carries"
        );
        assert!(!terms.registered_class_ids.contains(&class_id) && !terms.registered_artifact_roots.contains(&spec.root));
    }
    let collateral_before = state.bond(&env.chain.bonds[card]).expect("the registrant bond").collateral;
    // The gate, before the fee is spent.
    let certified = state.chain_certified_families(PalwCertifiedLaneV1::Attempt);
    let (entry, record) = palw_tir_registration_preflight_at_v1(&env.config.params, &env.bundle, &object, daa, &certified)
        .expect("the offline gate admits the class");
    assert_eq!(entry.class_id, class_id);
    let carrier = carrier_of(&env, &object, card, CARRIER_FEE);
    mempool_verdict(&env, &carrier).expect("the mempool takes the registration carrier");
    let carrying = env.chain.heartbeat(ttpb, vec![carrier.clone()]).await;
    assert!(carrying.transactions.iter().any(|tx| tx.id() == carrier.id()), "the node's own template carries the registration");
    let folding = env.chain.heartbeat(ttpb, Vec::new()).await;
    Mined {
        env,
        object,
        class_id,
        card,
        carrier,
        blocks: vec![h1, carrying, folding],
        record,
        activation: daa + ACTIVATION_AHEAD,
        collateral_before,
    }
}

#[tokio::test]
async fn g14_registration_mined_end_to_end_on_the_real_node_path() {
    mined_end_to_end(&case("moe-top2-shared"), 2).await;
}

/// The whole pipeline for one class, `c`, registered by card `card`'s bond (see the module doc).
async fn mined_end_to_end(c: &Case, card: usize) {
    mined_end_to_end_on(Env::new(0), c, card).await;
}

/// [`mined_end_to_end`] on a chain the caller built (the ruleset it likes, warmed up as far as it likes).
async fn mined_end_to_end_on(env: Env, c: &Case, card: usize) {
    use kaspa_consensus_core::palw_state_v2::PalwClassStatusV2;
    kaspa_core::log::try_init_logger("warn");
    let mut m = mine_registration_on(env, c, card).await;
    let env = &mut m.env;
    let bond = env.chain.bonds[m.card];
    let consensus = &env.chain.ctx.consensus;

    // ---- the persisted tip (read back from the store, as a restart reads it) ----
    let (fold_block, state) = env.chain.tip_state();
    assert_eq!(fold_block, m.blocks[2].header.hash, "the tip is the folding block");
    let row = state.class(&m.class_id).expect("the folding block wrote the class row");
    assert_eq!(
        row.status,
        PalwClassStatusV2::Registered { activation_daa: m.activation, pending_share_permille: 0 },
        "dormant until its activation"
    );
    assert_eq!(row.registrant_bond, Some(bond), "whose bond paid");
    assert_eq!(
        row.artifact_root,
        match &m.object {
            Obj::ClassRegisteredTirV1 { artifact_root, .. } => *artifact_root,
            _ => unreachable!(),
        }
    );
    assert_eq!(state.tir_class_v1(&m.class_id), Some(&m.record), "the tir_classes row is exactly what the gate derived");
    assert!(state.tir_class_v1(&m.class_id).unwrap().check_program_v1().is_ok(), "and the program travels with it");
    let (target, ..) = env.terms(&state);
    assert_eq!(state.class_target(&m.class_id).map(|t| t.target), Some(target), "at the chain's target");
    let b = state.bond(&bond).expect("the bond");
    assert!(state.registration_exposure(&bond) > 0, "the registration exposure is reserved on the registrant's bond");
    assert_eq!(
        b.collateral,
        m.collateral_before - kaspa_consensus_core::palw_state_v2::PALW_CLASS_REGISTRATION_BURN_SOMPI_V1,
        "1 MSK burned"
    );
    // The carrying block's own tip state (one block earlier) did not hold it: the fold is the chain block's.
    // ---- the node's reads (what the RPC serves) ----
    let rows = consensus.palw_v2_class_table();
    let served = rows.iter().find(|r| r.class_id == m.class_id).expect("getPalwClasses lists the class");
    assert!(served.status.contains("Registered"), "status: {}", served.status);
    assert_eq!(served.share_permille, None, "a pending class holds no share row yet");
    assert_eq!(served.artifact_root, row.artifact_root);
    assert_eq!(consensus.palw_tir_class_record_v1(m.class_id).as_ref(), Some(&m.record), "getPalwTirClass");
    let terms = consensus.palw_v2_registration_terms().expect("terms");
    assert!(terms.registered_class_ids.contains(&m.class_id) && terms.registered_artifact_roots.contains(&row.artifact_root));
    let registry = consensus.palw_model_registry_v1().expect("the registry read");
    let reg_row = registry.classes.iter().find(|c| c.class_id == m.class_id);
    eprintln!("[g14] registry row of the new class: {:?}", reg_row.map(|c| (c.row.clone(), c.share_permille, c.reason.clone())));

    // ---- registered is not eligible: the class exists and mines nothing (ADR-0145 §7) ----
    if let Some(c) = reg_row {
        let lifecycle = c.row.as_ref().map(|r| r.state.clone());
        assert!(
            lifecycle.as_ref().is_some_and(|st| !st.admits_claims()),
            "a registered class's lifecycle admits no claims: {lifecycle:?}"
        );
    } else {
        panic!("the registry reads a row for the new class");
    }
    let facts = consensus.palw_producer_facts_v2(m.class_id, Some(bond.0)).expect("producer facts for the new class");
    let verdict = facts.ready_to_produce(&TestConsensus::palw_v2_registry_keypair(m.card as u64).verification_key.as_ref().to_vec());
    eprintln!("[g14] the registrant's producer facts for its own new class: ready_to_produce = {verdict:?}");
    assert!(verdict.is_err(), "the registrant cannot produce in the class it just registered (dormant / Candidate)");

    // ---- the registration-status reads the RPC builds a verdict from ----
    let accepting_daa = row.registered_daa;
    let found =
        kaspa_consensus_core::palw_model_registration_v1::palw_registration_row_written_by_v1(&m.carrier, accepting_daa, &rows);
    assert_eq!(
        found.map(|r| r.class_id),
        Some(m.class_id),
        "getPalwModelRegistrationStatus <carrier txid> finds the row the carrier wrote (IR carriers included)"
    );
    assert!(
        kaspa_consensus_core::palw_model_registration_v1::palw_registration_carrier_object_v1(&m.carrier).is_some(),
        "the dropped-carrier diagnosis can re-read an IR carrier's object"
    );

    // ---- the clock flips it Active; the mempool no longer holds the spent float ----
    let ttpb = env.config.params.target_time_per_block();
    while env.chain.daa_of(env.chain.sink()) < m.activation + 1 {
        env.chain.heartbeat(ttpb, Vec::new()).await;
    }
    let (_, later) = env.chain.tip_state();
    assert_eq!(later.class(&m.class_id).map(|c| c.status.clone()), Some(PalwClassStatusV2::Active), "Active past its activation");
    assert_eq!(later.class_share_permille(&m.class_id), Some(0), "a registered class earns nothing by registering");
    assert!(
        matches!(mempool_verdict(env, &m.carrier), Err(kaspa_consensus_core::errors::tx::TxRuleError::MissingTxOutpoints)),
        "the mempool refuses a replay of the spent carrier"
    );
}

// ---- more than one node: replay, reorg, pruned import ---------------------------------------------------

use crate::model::stores::ghostdag::GhostdagStoreReader;
use kaspa_consensus_core::block::Block;

/// Every selected-chain block of `chain` from genesis (exclusive) to `upto`, oldest first.
pub(super) fn chain_blocks(chain: &T12Chain, upto: BlockHash) -> Vec<Block> {
    let vp = chain.vp();
    let genesis = chain.config.params.genesis.hash;
    let mut hashes = Vec::new();
    let mut at = upto;
    while at != genesis {
        hashes.push(at);
        at = vp.ghostdag_store.get_selected_parent(at).expect("a chain block has a selected parent");
    }
    hashes.reverse();
    hashes.into_iter().map(|h| chain.ctx.consensus.get_block(h).expect("the node holds its chain")).collect()
}

/// `block` arrives at `chain` as a peer's block does.
pub(super) async fn arrive(chain: &T12Chain, block: Block, what: &str) {
    let hash = block.header.hash;
    chain
        .ctx
        .consensus
        .validate_and_insert_block(block)
        .virtual_state_task
        .await
        .unwrap_or_else(|e| panic!("{what} {hash} was refused: {e}"));
}

fn fresh_node(m: &Mined) -> T12Chain {
    t12_genesis_chain(&m.env.config, &m.env.bundle, &m.env.premine, &m.env.floats)
}

pub(super) fn root_at(chain: &T12Chain, block: BlockHash) -> Hash64 {
    chain.vp().palw_state_v2_store.read().state_root_of(block).expect("a delta row")
}

/// A's chain to a block past the activation, returning every block after the warm-up.
async fn extend_past_activation(m: &mut Mined, extra: u64) {
    let ttpb = m.env.config.params.target_time_per_block();
    while m.env.chain.daa_of(m.env.chain.sink()) < m.activation + extra {
        let b = m.env.chain.heartbeat(ttpb, Vec::new()).await;
        m.blocks.push(b);
    }
}

#[tokio::test]
async fn g14_registration_replay_on_a_second_node_and_across_a_reorg() {
    kaspa_core::log::try_init_logger("warn");
    let mut a = mine_registration(&case("moe-top2-shared"), 2).await;
    let (h1, carrying, folding) = (a.blocks[0].clone(), a.blocks[1].clone(), a.blocks[2].clone());
    let float_outpoint = a.env.floats[a.card].0;
    let change = TransactionOutpoint::new(a.carrier.id(), 0);
    let bond = a.env.chain.bonds[a.card];

    // ---- Z replays A's chain (the way a syncing node does): same sink, same roots, same row ----
    let mut z = fresh_node(&a);
    for b in chain_blocks(&a.env.chain, a.env.chain.sink()) {
        arrive(&z, b, "A's block").await;
    }
    assert_eq!(z.sink(), a.env.chain.sink());
    let (_, at_a) = a.env.chain.tip_state();
    let (_, z_a) = z.tip_state();
    assert_eq!(z_a.state_root(), at_a.state_root(), "Z on A: A's root");
    assert_eq!(z_a.class(&a.class_id), at_a.class(&a.class_id), "Z on A: A's class row");
    assert_eq!(z_a.tir_class_v1(&a.class_id), Some(&a.record), "Z on A: the gate's record");
    for blk in [&h1, &carrying, &folding] {
        assert_eq!(root_at(&z, blk.header.hash), root_at(&a.env.chain, blk.header.hash), "per-block delta root {}", blk.header.hash);
    }
    assert!(
        z.ctx.consensus.get_virtual_utxo_entry(change).is_some() && z.ctx.consensus.get_virtual_utxo_entry(float_outpoint).is_none()
    );
    let collateral_on_a = z_a.bond(&bond).unwrap().collateral;
    assert_eq!(collateral_on_a, a.collateral_before - kaspa_consensus_core::palw_state_v2::PALW_CLASS_REGISTRATION_BURN_SOMPI_V1);

    // ---- B, from the same h1, mines a heavier chain that never saw the carrier ----
    let mut b = fresh_node(&a);
    arrive(&b, h1.clone(), "h1").await;
    b.ctx.simulated_time = h1.header.timestamp;
    let ttpb = a.env.config.params.target_time_per_block();
    let mut b_blocks = Vec::new();
    for _ in 0..4 {
        b_blocks.push(b.heartbeat(ttpb, Vec::new()).await);
    }
    assert!(b.tip_state().1.class(&a.class_id).is_none(), "B never registered it");
    for blk in &b_blocks {
        arrive(&z, blk.clone(), "B's block").await;
    }
    assert_eq!(z.sink(), b.sink(), "B out-works A's two blocks: Z reorgs onto B");
    let (_, z_b) = z.tip_state();
    assert_eq!(z_b.state_root(), b.tip_state().1.state_root(), "Z on B: the root of B's fresh replay");
    assert!(z_b.class(&a.class_id).is_none() && z_b.tir_class_v1(&a.class_id).is_none(), "the reorged-out registration left no row");
    assert_eq!(z_b.bond(&bond).unwrap().collateral, a.collateral_before, "its burn is returned with it");
    assert_eq!(z_b.registration_exposure(&bond), 0, "and its exposure");
    // **The sink's PALW state is the sink chain's; the virtual's UTXO view also merges A's blue blocks.** The carrier
    // (in A's carrying block) is therefore accepted by the VIRTUAL — its float is spent, its change stands — while the
    // class row exists only once a chain block folds that acceptance.
    assert!(
        z.ctx.consensus.get_virtual_utxo_entry(change).is_some() && z.ctx.consensus.get_virtual_utxo_entry(float_outpoint).is_none()
    );
    assert!(z.ctx.consensus.palw_v2_class_table().iter().all(|r| r.class_id != a.class_id), "getPalwClasses no longer lists it");
    assert_eq!(z.ctx.consensus.palw_tir_class_record_v1(a.class_id), None);
    assert!(
        matches!(mempool_verdict_on(&z, &a.carrier), Err(kaspa_consensus_core::errors::tx::TxRuleError::MissingTxOutpoints)),
        "the float is spent in the virtual (merged), so the pool refuses a second carrier"
    );
    for blk in &b_blocks {
        assert_eq!(root_at(&z, blk.header.hash), root_at(&b, blk.header.hash), "Z keeps B's delta row");
    }
    // A's delta rows are kept on Z, reverted (the reorg walk needs them again).
    assert_eq!(root_at(&z, folding.header.hash), root_at(&a.env.chain, folding.header.hash));

    // ---- the next chain block on B's side MERGES A's blue blocks and folds the carrier: the registration is not lost
    // by the reorg, it is re-folded (at that block's DAA, under the same signature and the same gate) ----
    z.ctx.simulated_time = b.ctx.simulated_time;
    let z1 = z.heartbeat(ttpb, Vec::new()).await;
    let (_, z_merged) = z.tip_state();
    let merged = z_merged.class(&a.class_id).expect("the merged carrier is folded by Z's next chain block");
    assert_eq!(merged.registered_daa, z.daa_of(z1.header.hash), "registered at the folding block's DAA");
    assert_eq!(z_merged.tir_class_v1(&a.class_id), Some(&a.record), "with the gate's record");
    assert_eq!(z_merged.bond(&bond).unwrap().collateral, collateral_on_a, "and the burn is taken again, once");

    // ---- A out-works B again: Z reorgs back, the row returns identically ----
    extend_past_activation(&mut a, 3).await;
    let a_tail: Vec<Block> = a.blocks[3..].to_vec();
    assert!(a_tail.len() > 5, "A's tail ({}) out-works B's four blocks and Z's merge block", a_tail.len());
    for blk in &a_tail {
        arrive(&z, blk.clone(), "A's block").await;
    }
    assert_eq!(z.sink(), a.env.chain.sink(), "Z back on A");
    let (_, z_again) = z.tip_state();
    let (_, a_end) = a.env.chain.tip_state();
    assert_eq!(z_again.state_root(), a_end.state_root(), "Z on A again: A's fresh-replay root");
    assert_eq!(z_again.class(&a.class_id), a_end.class(&a.class_id), "the same class row, now Active");
    assert_eq!(z_again.tir_class_v1(&a.class_id), Some(&a.record));
    assert_eq!(z_again.bond(&bond).unwrap().collateral, collateral_on_a);
    assert!(matches!(mempool_verdict_on(&z, &a.carrier), Err(kaspa_consensus_core::errors::tx::TxRuleError::MissingTxOutpoints)));
    for blk in &a.blocks {
        assert_eq!(
            root_at(&z, blk.header.hash),
            root_at(&a.env.chain, blk.header.hash),
            "Z on A again: delta root {}",
            blk.header.hash
        );
    }
}

fn mempool_verdict_on(chain: &T12Chain, tx: &Transaction) -> Result<(), kaspa_consensus_core::errors::tx::TxRuleError> {
    chain
        .vp()
        .validate_mempool_transaction(&mut kaspa_consensus_core::tx::MutableTransaction::from_tx(tx.clone()), &Default::default())
}

/// **A pruned join inside the registration's life**: the importer follows A through P (after the fold, before the
/// activation), sees the header of P's child, is left as a pruned join leaves a node (no PALW tip, no delta below P),
/// and installs the carriage A serves. Everything it knows of the class then came through the carriage — and the
/// activation clock, the folds that follow and A's blocks all give A's roots.
#[tokio::test]
async fn g14_registration_survives_a_pruned_import() {
    use kaspa_consensus_core::palw_state_v2::{PalwClassStatusV2, PalwStateCarriageV2};
    kaspa_core::log::try_init_logger("warn");
    let mut a = mine_registration(&case("sliding-global"), 4).await;
    extend_past_activation(&mut a, 3).await;
    let all = chain_blocks(&a.env.chain, a.env.chain.sink());
    // P: the folding block + 2 (the class is Registered, activation is ~28 DAA away).
    let k = all.iter().position(|b| b.header.hash == a.blocks[2].header.hash).unwrap() + 2;
    let (p, t) = (all[k].header.hash, all[k + 1].clone());
    let importer = fresh_node(&a);
    for b in &all[..=k] {
        arrive(&importer, b.clone(), "a block through the pruning point").await;
    }
    arrive(&importer, Block::from_header_arc(t.header.clone()), "T's header").await;
    let vp = a.env.chain.vp();
    vp.capture_pruning_point_palw_state(p);
    let wire = borsh::to_vec(&vp.pruning_point_palw_state(p).expect("servable")).expect("serializes");
    let carriage: PalwStateCarriageV2 = borsh::from_slice(&wire).expect("the wire bytes decode");
    {
        let ivp = importer.vp();
        let mut store = ivp.palw_state_v2_store.write();
        store.delete_tip_for_tests().expect("no PALW tip");
        for blk in std::iter::once(importer.config.params.genesis.hash).chain(all[..=k].iter().map(|b| b.header.hash)) {
            store.delete_delta_for_tests(blk).expect("no delta row at or below the pruning point");
        }
    }
    importer.vp().import_pruning_point_palw_state(p, carriage).expect("the served carriage installs against T's committed root");
    let (at, imported) = importer.tip_state();
    assert_eq!(at, p);
    assert_eq!(imported.state_root(), root_at(&a.env.chain, p), "the imported state is P's, root for root");
    let row = imported.class(&a.class_id).expect("the class came through the carriage");
    assert!(matches!(row.status, PalwClassStatusV2::Registered { .. }), "still dormant at P: {:?}", row.status);
    assert_eq!(imported.tir_class_v1(&a.class_id), Some(&a.record), "with its program");
    assert!(imported.tir_class_v1(&a.class_id).unwrap().check_program_v1().is_ok());
    assert!(importer.ctx.consensus.palw_v2_class_table().iter().any(|r| r.class_id == a.class_id), "and it is served");
    // The rest of A's chain, folded by the importer from the imported state: the activation flip included.
    for blk in &all[k + 1..] {
        arrive(&importer, blk.clone(), "A's block after P").await;
        assert_eq!(importer.sink(), blk.header.hash);
        assert_eq!(
            importer.tip_state().1.state_root(),
            root_at(&a.env.chain, blk.header.hash),
            "the importer folds to A's root at {}",
            blk.header.hash
        );
    }
    let (_, end) = importer.tip_state();
    assert_eq!(
        end.class(&a.class_id).map(|c| c.status.clone()),
        Some(PalwClassStatusV2::Active),
        "the importer flipped the class Active itself"
    );
}

// ---- mined: what the node does with a registration the gate would refuse --------------------------------------

/// Carry `tx` in the node's own template and let the next chain block fold it: returns the two blocks.
async fn mine_carrier(chain: &mut T12Chain, tx: &Transaction) -> (Block, Block) {
    let ttpb = chain.config.params.target_time_per_block();
    let carrying = chain.heartbeat(ttpb, vec![tx.clone()]).await;
    assert!(carrying.transactions.iter().any(|t| t.id() == tx.id()), "the template carries the registration carrier");
    let folding = chain.heartbeat(ttpb, Vec::new()).await;
    (carrying, folding)
}

/// **Below `palw_tir_v1` an IR registration is a carrier the chain mines and ignores; the same object, re-carried past the
/// fence, registers.** The signature does not bind the height, so nothing is re-signed: the gate's `daa` and the chain's
/// block DAA are the only clock.
#[tokio::test]
async fn g14_registration_mined_below_the_fence_is_dropped_by_name_then_accepted_past_it() {
    kaspa_core::log::try_init_logger("warn");
    const FENCE: u64 = 8;
    let mut env = Env::new(FENCE);
    let ttpb = env.config.params.target_time_per_block();
    env.chain.heartbeat(ttpb, Vec::new()).await;
    let (block, state) = env.chain.tip_state();
    let daa = env.chain.daa_of(block);
    let object = signed(&env, &state, &spec_of(&case("moe-top2-shared"), 3, 100));
    let class_id = class_id_of(&object);
    assert_eq!(
        palw_tir_registration_preflight_at_v1(&env.config.params, &env.bundle, &object, daa, &[]).map(|_| ()),
        Err(E::TirNeedsItsFence),
        "the gate names the fence at the tip"
    );
    // The mempool does not run the gate (it runs the lifecycle shape and the funding): the carrier is admitted.
    let first = carrier_of(&env, &object, 3, CARRIER_FEE);
    mempool_verdict(&env, &first)
        .expect("the mempool admits a registration the gate refuses (node policy asks nothing of an IR registration)");
    let (carrying, folding) = mine_carrier(&mut env.chain, &first).await;
    assert!(env.chain.daa_of(folding.header.hash) < FENCE);
    let _ = &carrying;
    let (_, after) = env.chain.tip_state();
    assert!(after.class(&class_id).is_none() && after.tir_class_v1(&class_id).is_none(), "dropped by name below the fence");
    assert_eq!(after.bond(&env.chain.bonds[3]).unwrap().collateral, state.bond(&env.chain.bonds[3]).unwrap().collateral, "no burn");
    assert!(env.chain.ctx.consensus.get_virtual_utxo_entry(TransactionOutpoint::new(first.id(), 0)).is_some(), "but the fee is spent");
    // The status readers: mined, change stands, no row, two DAA on — a dropped carrier, and the gate's reason is re-derivable.
    let change_daa = env.chain.ctx.consensus.get_virtual_utxo_entry(TransactionOutpoint::new(first.id(), 0)).unwrap().block_daa_score;
    while env.chain.ctx.consensus.get_virtual_daa_score()
        < change_daa + kaspa_consensus_core::palw_model_registration_v1::PALW_REGISTRATION_DROP_SETTLE_DAA_V1
    {
        env.chain.heartbeat(ttpb, Vec::new()).await; // the settle margin: the fold of the accepting block has certainly run
    }
    eprintln!(
        "[g14] change entry daa {change_daa}, virtual daa {}, sink daa {}, carrying {} folding {}",
        env.chain.ctx.consensus.get_virtual_daa_score(),
        env.chain.daa_of(env.chain.sink()),
        env.chain.daa_of(carrying.header.hash),
        env.chain.daa_of(folding.header.hash)
    );
    assert!(
        kaspa_consensus_core::palw_model_registration_v1::palw_registration_carrier_dropped_v1(
            false,
            false,
            Some(change_daa),
            env.chain.ctx.consensus.get_virtual_daa_score()
        )
        .is_some()
    );
    let reread =
        kaspa_consensus_core::palw_model_registration_v1::palw_registration_carrier_object_v1(&first).expect("an IR carrier re-reads");
    assert_eq!(reread, object);
    assert_eq!(
        palw_tir_registration_preflight_at_v1(&env.config.params, &env.bundle, &reread, change_daa, &[]).map(|_| ()),
        Err(E::TirNeedsItsFence),
        "re-asked at the accepting DAA, the gate gives the chain's reason"
    );

    // Past the fence: the SAME object, a new carrier funded by the first one's change.
    while env.chain.daa_of(env.chain.sink()) < FENCE + 1 {
        env.chain.heartbeat(ttpb, Vec::new()).await;
    }
    let (block, state) = env.chain.tip_state();
    check("past the fence", &judge(&env, &state, block, env.chain.daa_of(block), &object), Want::Accept).unwrap();
    let second = carrier_from(&env, &object, 3, change_of(&first, 3), CARRIER_FEE);
    mempool_verdict(&env, &second).expect("the second carrier");
    mine_carrier(&mut env.chain, &second).await;
    let (_, registered) = env.chain.tip_state();
    assert!(registered.class(&class_id).is_some(), "registered past the fence under the signature made before it");
}

/// **A registration the chain will drop still costs a carrier fee: the mempool admits it and the template mines it.**
/// Each refused object below is judged by the gate (or, for the chain-state reasons, by the arm); mined, the block stands,
/// the class row is never written, and the carrier's change is the only trace.
#[tokio::test]
async fn g14_refused_registrations_cost_a_fee_and_write_nothing() {
    kaspa_core::log::try_init_logger("warn");
    let mut env = Env::new(0);
    let ttpb = env.config.params.target_time_per_block();
    env.chain.heartbeat(ttpb, Vec::new()).await;
    let (block, state) = env.chain.tip_state();
    let daa = env.chain.daa_of(block);
    let dense = case("dense-gqa-2layer");
    let s = spec_of(&dense, 1, daa + 5);
    let (target, ..) = env.terms(&state);
    let genesis = env.config.params.genesis.hash;
    let refused: Vec<(&str, Obj, Option<&str>)> = vec![
        ("unknown op tag", mutate_class(&env, &state, &s, |c| c.program[40] = 0xFE), Some("TIR_PROGRAM_REFUSED")),
        (
            "stale id (tokenizer)",
            stale_id(&env, &state, &s, |c, _| c.tokenizer_id = Hash64::from_bytes([1; 64])),
            Some("TIR_CLASS_ID_IS_NOT_DERIVED"),
        ),
        (
            "share claim",
            edit_raw(&env, &state, &s, |o| reg!(o, { share_permille } => *share_permille = 1)),
            Some("NOT_END_TO_END_CERTIFIED"),
        ),
        (
            "replayed from another network",
            {
                let mut o = build(&env, &state, &s);
                sign(&mut o, 1, network_domain("kaspa-testnet-11", genesis));
                o
            },
            None,
        ),
        ("wrong target", edit_raw(&env, &state, &s, |o| reg!(o, { initial_target } => *initial_target = target / 2)), None),
    ];
    let mut funding = env.floats[5].clone();
    for (name, object, gate_code) in refused {
        let tx = carrier_from(&env, &object, 5, funding.clone(), CARRIER_FEE);
        mempool_verdict(&env, &tx).unwrap_or_else(|e| panic!("{name}: the mempool takes the carrier ({e})"));
        let (_, before) = env.chain.tip_state();
        let (carrying, folding) = mine_carrier(&mut env.chain, &tx).await;
        let mined_daa = env.chain.daa_of(carrying.header.hash);
        let (_, after) = env.chain.tip_state();
        assert!(after.class(&class_id_of(&object)).is_none(), "{name}: no class row");
        assert_eq!(
            after.bond(&env.chain.bonds[1]).unwrap().collateral,
            before.bond(&env.chain.bonds[1]).unwrap().collateral,
            "{name}: no burn"
        );
        assert_eq!(
            after.registration_exposure(&env.chain.bonds[1]),
            before.registration_exposure(&env.chain.bonds[1]),
            "{name}: no exposure"
        );
        assert_eq!(env.chain.sink(), folding.header.hash, "{name}: the block stands");
        // The RPC's diagnosis of a dropped carrier: the gate re-asked at the accepting DAA. It names the gate's reasons;
        // for the chain-state ones (signature, target) it can only say the gate admits.
        let reread = kaspa_consensus_core::palw_model_registration_v1::palw_registration_carrier_object_v1(&tx).expect("re-read");
        let rediagnosed = palw_tir_registration_preflight_at_v1(&env.config.params, &env.bundle, &reread, mined_daa, &[]);
        match gate_code {
            Some(code) => {
                assert_eq!(rediagnosed.as_ref().err().map(|e| e.code()), Some(code), "{name}: the diagnosis names the gate's code")
            }
            None => assert!(rediagnosed.is_ok(), "{name}: the diagnosis cannot see a chain-state refusal ({rediagnosed:?})"),
        }
        eprintln!(
            "[g14] mined+dropped {name:<32} mempool=Ok block stands, class absent, gate re-diagnosis={:?}",
            rediagnosed.map(|_| ()).map_err(|e| e.code())
        );
        funding = change_of(&tx, 5);
    }
}

// ---- the whole release schedule ---------------------------------------------------------------------------

/// **The gate and the chain, at every height the shipped testnet-12 schedule changes a rule.** Every fence of
/// `palw_t12_shipped_params()` (the harness cards on top) is crossed: for each height `h` of the schedule the
/// corpus classes are judged at `h - 1`, `h` and `h + 1` by `palw_tir_registration_preflight_at_v1` and by the
/// processor's arm (rules resolved by the PROCESSOR at that height: the court, the held context, the demand rules, the
/// model court window). On the genesis state, with the registration signed for the height, the chain's only open
/// questions are the gate's — so they must agree everywhere.
#[tokio::test]
async fn g14_gate_and_chain_agree_across_the_shipped_fence_schedule() {
    use super::t12_round_lane_e2e::t12_with_harness_cards_over;
    let over = t12_with_harness_cards_over(kaspa_consensus_core::config::params::palw_t12_shipped_params(), false);
    let env = Env::over(over);
    let (block, state) = env.chain.tip_state();
    let schedule = env.config.params.fence_schedule_v1();
    let mut heights: Vec<u64> = vec![0, 1];
    for h in &schedule {
        heights.extend([h.saturating_sub(1), *h, h + 1]);
    }
    heights.push(schedule.last().copied().unwrap_or(0) + 1_000);
    heights.sort_unstable();
    heights.dedup();
    let tir_at = env.config.params.palw_tir_v1_fence().map(|f| f.activation.daa_score());
    eprintln!(
        "[g14] shipped schedule: {} fence heights {:?}; palw_tir_v1 at {:?}; judging at {} heights",
        schedule.len(),
        schedule,
        tir_at,
        heights.len()
    );
    // Wider contexts bite the court's window, the ladder and the dissection fences; they are the classes whose verdict can
    // move with the height.
    let mut corpus: Vec<Case> = [64, 512, 4_096, 32_000].into_iter().flat_map(corpus_at).collect();
    corpus.extend(real_cases());
    let mut bad = Vec::new();
    let mut verdicts: BTreeMap<u64, (u32, u32)> = BTreeMap::new();
    let mut moved: BTreeMap<String, Vec<(u64, bool)>> = BTreeMap::new();
    for &daa in &heights {
        for (i, c) in corpus.iter().enumerate() {
            let o = signed(&env, &state, &spec_of(c, 1 + i % 6, daa));
            let j = judge(&env, &state, block, daa, &o);
            let entry = verdicts.entry(daa).or_default();
            moved.entry(format!("{}@{}", c.name, c.class.layout.max_context)).or_default().push((daa, j.preflight.is_ok()));
            match (&j.preflight, &j.validate) {
                (Ok(()), Ok(())) if j.fold.as_ref().is_some_and(|f| f.is_ok()) && j.accepted => entry.0 += 1,
                (Err(e), Err(why))
                    if why.contains(&e.to_string())
                        || (matches!(e, E::TirNeedsItsFence) && why.contains("palw_tir_v1 is not in force")) =>
                {
                    entry.1 += 1
                }
                _ => bad.push(format!("daa {daa} {}: {j:?}", c.name)),
            }
        }
    }
    eprintln!("[g14] (admitted by both, refused by both) per height: {verdicts:?}");
    for (name, history) in &moved {
        let flips: Vec<_> = history.windows(2).filter(|w| w[0].1 != w[1].1).map(|w| (w[0].0, w[1].0, w[1].1)).collect();
        if !flips.is_empty() {
            eprintln!("[g14] verdict of {name} flips (from daa, to daa, admitted): {flips:?}");
        }
    }
    assert!(bad.is_empty(), "the gate and the chain disagree at a fence height:\n{}", bad.join("\n"));
    if let Some(at) = tir_at {
        if at > 0 {
            assert!(verdicts.get(&(at - 1)).is_some_and(|v| v.0 == 0), "nothing registers below palw_tir_v1");
        }
        assert!(verdicts.get(&at).is_some_and(|v| v.0 >= 5), "at palw_tir_v1 the admissible corpus classes register");
    }
}

// ---- classes lowered from REAL local checkpoints (shape-only: headers, no weights) --------------------------

/// The classes `misaka-palw-sdk/tests/g14_registration_fixture.rs` lowered from the local Hugging Face checkpoints: the
/// program the preflight's shape-only lowering builds from `config.json` and the safetensors header, the layout the SDK's
/// layout search chose under THIS harness's ruleset, the tokenizer id of the checkpoint's `tokenizer.json`. The artifact
/// root is synthetic (no weights were loaded) — registration never reads weights.
fn real_cases() -> Vec<Case> {
    real_cases_in("")
}

/// [`real_cases`] from `fixtures/g14/<sub>` (`shipped`: lowered and judged under `palw_t12_shipped_params()` at DAA 5,585).
fn real_cases_in(sub: &str) -> Vec<Case> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/pipeline/virtual_processor/tests/fixtures/g14").join(sub);
    if !dir.is_dir() {
        return Vec::new();
    }
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("the fixtures directory")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();
    files
        .into_iter()
        .map(|path| {
            let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            let h64 = |k: &str| {
                let b: [u8; 64] = unhex(v[k].as_str().unwrap()).try_into().expect("64 bytes");
                Hash64::from_bytes(b)
            };
            let class = PalwTirClassV1 {
                version: PALW_TIR_CLASS_VERSION_V1,
                program: unhex(v["program_borsh_hex"].as_str().unwrap()),
                layout: borsh::from_slice(&unhex(v["layout_borsh_hex"].as_str().unwrap())).expect("a layout"),
                tokenizer_id: h64("tokenizer_id_hex"),
            };
            let root = h64("artifact_root_hex");
            assert_eq!(class.class_id(&root), h64("class_id_hex"), "{}: the fixture's class id is the derived one", path.display());
            assert_eq!(v["weights_loaded"], false);
            Case { name: format!("real:{}", v["name"].as_str().unwrap()), class, root, real_root: false }
        })
        .collect()
}

/// **Classes lowered from real checkpoints: the offline gate and the chain, then the whole pipeline.** Each fixture is the
/// widest context the harness's gate admitted (the SDK's layout search); the chain must keep it, and every adversarial
/// edit of it must be judged alike by both.
#[tokio::test]
async fn g14_real_checkpoint_classes_parity_and_mined() {
    let cases = real_cases();
    assert!(!cases.is_empty(), "the fixtures exist");
    let env = Env::new(0);
    let (block, state) = env.chain.tip_state();
    let daa = env.chain.daa_of(block);
    for (i, c) in cases.iter().enumerate() {
        let s = spec_of(c, 1 + i % 6, daa);
        let o = signed(&env, &state, &s);
        let j = judge(&env, &state, block, daa, &o);
        eprintln!(
            "[g14] {:<34} context {:>6} program {:>6} B: gate {:?} chain_accepted {}",
            c.name,
            c.class.layout.max_context,
            c.class.program.len(),
            j.preflight,
            j.accepted
        );
        check(&c.name, &j, Want::Accept).unwrap();
        // The same edits as the corpus table, on a real class.
        let mamba = case("mamba2");
        let cases: Vec<(&str, Obj, Want)> = vec![
            ("real: a trailing byte", mutate_class(&env, &state, &s, |k| k.program.push(0)), Want::Refuse("TIR_PROGRAM_REFUSED")),
            (
                "real: tokenizer swapped, id stale",
                stale_id(&env, &state, &s, |k, _| k.tokenizer_id = Hash64::from_bytes([1; 64])),
                Want::Refuse("TIR_CLASS_ID_IS_NOT_DERIVED"),
            ),
            (
                "real: root swapped, id stale",
                stale_id(&env, &state, &s, |_, r| *r = Hash64::from_bytes([2; 64])),
                Want::Refuse("TIR_CLASS_ID_IS_NOT_DERIVED"),
            ),
            (
                "real: another program's layout",
                mutate_class(&env, &state, &s, |k| k.layout = layout_of(&mamba.class, 64)),
                Want::Refuse("TIR_CLASS_REFUSED"),
            ),
            (
                "real: share claim",
                edit_raw(&env, &state, &s, |o| reg!(o, { share_permille } => *share_permille = 1)),
                Want::Refuse("NOT_END_TO_END_CERTIFIED"),
            ),
            ("real: unsigned", build(&env, &state, &s), Want::StatefulDrop("is not signed by the bond it names")),
        ];
        run_table(&env, &state, block, daa, cases);
    }
    for (i, c) in cases.iter().enumerate() {
        mined_end_to_end(c, 1 + i % 6).await;
    }
}

/// **The same weights under a second class id are not refused by the chain.** `PalwRegistrationTermsV2::registered_artifact_roots`
/// is a CLIENT-side filter (the SDK skips a root the chain holds); `claim_artifact_root` keys ownership by `(class_id, root)`,
/// so a registration of a held root under another tokenizer / layout — another class id — passes both the gate and the fold.
/// Recorded as what the code does today (the doc of the terms calls it "never meaningful"), not as a defect of this lane.
#[tokio::test]
async fn g14_same_weights_under_another_class_id_are_accepted_by_chain_and_gate() {
    let env = Env::new(0);
    let (block, state) = env.chain.tip_state();
    let dense = case("dense-gqa-2layer");
    let first = signed(&env, &state, &spec_of(&dense, 1, 0));
    let vp = env.chain.vp();
    let point = env.point(block, 0);
    let after =
        vp.palw_v2_fold_accepted_for_tests(&state, &env.bundle.state, &point, std::slice::from_ref(&first)).expect("the first folds");
    assert!(after.classes_iter().any(|(_, c)| c.artifact_root == dense.root));
    let mut other = spec_of(&dense, 2, 0);
    other.class.tokenizer_id = Hash64::from_bytes([0x44; 64]);
    let second = signed(&env, &after, &other);
    assert_ne!(class_id_of(&second), class_id_of(&first), "another tokenizer is another class");
    check("same root, another tokenizer, another registrant", &judge(&env, &after, block, 0, &second), Want::Accept).unwrap();
}

/// **A model alias is a free claim.** The registration carries no name (the class id is the whole identity); the only
/// alias the chain stores is `ModelLineFounded.name`, signed by any active bond over a class that is Active, bound to no
/// property of the weights. A bond that registered nothing founds a line named after somebody else's model on this class.
#[tokio::test]
async fn g14_a_model_alias_is_a_free_claim_on_an_active_class() {
    use kaspa_consensus_core::palw_state_v2::PalwClassStatusV2;
    kaspa_core::log::try_init_logger("warn");
    let mut m = mine_registration(&case("moe-top2-shared"), 2).await;
    let ttpb = m.env.config.params.target_time_per_block();
    while m.env.chain.daa_of(m.env.chain.sink()) < m.activation + 1 {
        m.env.chain.heartbeat(ttpb, Vec::new()).await;
    }
    let (block, state) = m.env.chain.tip_state();
    assert_eq!(state.class(&m.class_id).map(|c| c.status.clone()), Some(PalwClassStatusV2::Active));
    let daa = m.env.chain.daa_of(block);
    let env = &m.env;
    let (founder_card, name) = (6usize, b"Meta-Llama-3-70B-Instruct".to_vec());
    let root = Hash64::from_bytes([0xA1; 64]);
    let founder = env.chain.bonds[founder_card];
    let message =
        kaspa_consensus_core::palw_model_lines_v1::palw_model_line_founded_message_v1(env.domain, &m.class_id, &name, &founder, &root);
    let key = TestConsensus::palw_v2_registry_keypair(founder_card as u64);
    let signature = libcrux_ml_dsa::ml_dsa_87::sign(
        &key.signing_key,
        message.as_byte_slice(),
        kaspa_consensus_core::palw_model_lines_v1::PALW_MODEL_LINE_MLDSA87_CONTEXT,
        [0x62u8; 32],
    )
    .expect("signs")
    .as_ref()
    .to_vec();
    let o = Obj::ModelLineFounded { class_id: m.class_id, name: name.clone(), founder, root, signature };
    let vp = env.chain.vp();
    let point = PalwBlockContextV2 { blue_score: state.last_point().map(|p| p.blue_score + 1).unwrap_or(1), ..env.point(block, daa) };
    let verdict = vp.palw_v2_validate_objects(&state, &env.bundle.state, &point, std::slice::from_ref(&o));
    let folded = vp.palw_v2_fold_accepted_for_tests(&state, &env.bundle.state, &point, std::slice::from_ref(&o));
    eprintln!("[g14] alias spoof: arm {verdict:?}; fold {:?}", folded.as_ref().map(|_| ()).map_err(|e| e.to_string()));
    // The assertion is the observation: whichever it is, it is recorded in registration-e2e-record.md.
    match (&verdict, &folded) {
        (Ok(()), Ok(after)) => {
            let line = kaspa_consensus_core::palw_model_lines_v1::model_line_id_v1(&m.class_id, &founder, &name);
            assert!(
                after.model_line(&line).is_some_and(|l| l.name == name),
                "the chain stores the spoofed alias against a class it has no relation to"
            );
        }
        _ => eprintln!(
            "[g14] the alias object is not expressible on this ruleset: {verdict:?} / {:?}",
            folded.as_ref().err().map(|e| e.to_string())
        ),
    }
}

/// **A real restart**: the node is shut down and a new `Consensus` is opened over the SAME database. Everything the new
/// process knows of the class — the sink, the PALW tip it loads, the class table, the `tir_classes` row, the delta rows,
/// the UTXO set — came off disk, and it carries on: it folds the activation flip and a second node that replays the whole
/// chain (the blocks from before the restart and the ones after it) reaches its roots.
#[tokio::test]
async fn g14_registration_survives_a_node_restart_over_the_same_database() {
    use kaspa_consensus_core::palw_state_v2::PalwClassStatusV2;
    use kaspa_database::{create_temp_db, prelude::ConnBuilder};
    kaspa_core::log::try_init_logger("warn");
    let parts = t12_tir_config(0);
    let (config, bundle) = (parts.0.clone(), parts.1.clone());
    let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let (sender, _rx) = async_channel::unbounded();
    let first = TestConsensus::with_db(db.clone(), &config, sender);
    let chain = t12_genesis_chain_on(first, &parts.0, &parts.1, &parts.2, &parts.3);
    let a = mine_registration_on(Env::around(chain, parts), &case("mamba2"), 5).await;
    let (class_id, record, activation, carrier_id) = (a.class_id, a.record.clone(), a.activation, a.carrier.id());
    let (sink, root) = (a.env.chain.sink(), a.env.chain.tip_state().1.state_root());
    let before_class = a.env.chain.tip_state().1.class(&class_id).cloned().expect("registered");
    let chain_before: Vec<Block> = chain_blocks(&a.env.chain, sink);
    let delta_roots: Vec<Hash64> = chain_before.iter().map(|b| root_at(&a.env.chain, b.header.hash)).collect();
    let (simulated_time, nonce) = (a.env.chain.ctx.simulated_time, a.env.chain.nonce_for_reopen());
    let (premine, floats) = (a.env.premine.clone(), a.env.floats.clone());

    // ---- stop the node ----
    {
        let Mined { env, .. } = a;
        let Env { chain, .. } = env;
        drop(chain); // TestContext's Drop shuts the processors down and releases the node's handles on the database
    }
    // ---- start it again on the same database ----
    // As a restarting node is configured: the database already holds its genesis (`process_genesis` off).
    let mut resumed = config.clone();
    resumed.process_genesis = false;
    let (sender, _rx2) = async_channel::unbounded();
    let second = TestConsensus::with_db(db.clone(), &resumed, sender);
    let mut r = t12_reopened_chain(second, &resumed, &bundle, simulated_time, nonce);
    assert_eq!(r.sink(), sink, "the restarted node's sink is the stopped node's");
    let (tip, state) = r.tip_state();
    assert_eq!((tip, state.state_root()), (sink, root), "the PALW tip it loads off disk");
    assert_eq!(state.class(&class_id), Some(&before_class), "the class row");
    assert_eq!(state.tir_class_v1(&class_id), Some(&record), "the tir_classes row, program included");
    assert!(state.tir_class_v1(&class_id).unwrap().check_program_v1().is_ok());
    assert!(r.ctx.consensus.palw_v2_class_table().iter().any(|row| row.class_id == class_id), "getPalwClasses");
    assert_eq!(r.ctx.consensus.palw_tir_class_record_v1(class_id).as_ref(), Some(&record), "getPalwTirClass");
    assert!(
        r.ctx.consensus.get_virtual_utxo_entry(TransactionOutpoint::new(carrier_id, 0)).is_some(),
        "the carrier's change, off disk"
    );
    for (b, want) in chain_before.iter().zip(&delta_roots) {
        assert_eq!(root_at(&r, b.header.hash), *want, "the delta row of {} survived", b.header.hash);
    }
    // ---- it carries on: the activation flip, folded by the restarted node ----
    let ttpb = config.params.target_time_per_block();
    let mut after = Vec::new();
    while r.daa_of(r.sink()) < activation + 2 {
        after.push(r.heartbeat(ttpb, Vec::new()).await);
    }
    let (_, later) = r.tip_state();
    assert_eq!(later.class(&class_id).map(|c| c.status.clone()), Some(PalwClassStatusV2::Active), "Active, flipped after the restart");
    assert_eq!(later.tir_class_v1(&class_id), Some(&record));
    // ---- and a node replaying the whole chain agrees with every root ----
    let z = t12_genesis_chain(&config, &bundle, &premine, &floats);
    for b in chain_blocks(&r, r.sink()) {
        arrive(&z, b, "a block of the restarted chain").await;
    }
    assert_eq!(z.sink(), r.sink());
    assert_eq!(z.tip_state().1.state_root(), later.state_root(), "the replaying node reaches the restarted node's root");
    for b in chain_before.iter().chain(&after) {
        assert_eq!(root_at(&z, b.header.hash), root_at(&r, b.header.hash), "delta {} across the restart", b.header.hash);
    }
}

/// **Two registrations in one template: the walk keeps one** (`PALW_TIR_REGISTRATION_MAX_PER_BLOCK_V1`) through the whole
/// pipeline — both carriers pass the mempool and ride the node's own block, the first in the block's transaction order is
/// folded, the second is dropped by name, the block stands, and both fees are spent.
#[tokio::test]
async fn g14_two_registrations_in_one_block_one_is_folded() {
    kaspa_core::log::try_init_logger("warn");
    let mut env = Env::new(0);
    let ttpb = env.config.params.target_time_per_block();
    env.chain.heartbeat(ttpb, Vec::new()).await;
    let (block, state) = env.chain.tip_state();
    let daa = env.chain.daa_of(block);
    let (a, b) = (
        signed(&env, &state, &spec_of(&case("dense-gqa-2layer"), 1, daa)),
        signed(&env, &state, &spec_of(&case("moe-top2-shared"), 2, daa)),
    );
    let (ca, cb) = (carrier_of(&env, &a, 1, CARRIER_FEE), carrier_of(&env, &b, 2, CARRIER_FEE));
    mempool_verdict(&env, &ca).expect("a");
    mempool_verdict(&env, &cb).expect("b");
    let carrying = env.chain.heartbeat(ttpb, vec![ca.clone(), cb.clone()]).await;
    let order: Vec<_> = carrying.transactions.iter().map(|t| t.id()).filter(|id| *id == ca.id() || *id == cb.id()).collect();
    assert_eq!(order.len(), 2, "both carriers ride the block");
    env.chain.heartbeat(ttpb, Vec::new()).await;
    let (_, after) = env.chain.tip_state();
    let (first, second) = if order[0] == ca.id() { (&a, &b) } else { (&b, &a) };
    assert!(after.class(&class_id_of(first)).is_some(), "the first in the block's order is folded");
    assert!(after.class(&class_id_of(second)).is_none(), "the second is dropped by name");
    assert!(env.chain.ctx.consensus.get_virtual_utxo_entry(TransactionOutpoint::new(ca.id(), 0)).is_some());
    assert!(env.chain.ctx.consensus.get_virtual_utxo_entry(TransactionOutpoint::new(cb.id(), 0)).is_some(), "both fees are spent");
}

/// **The widest class the SDK's preflight registers under the shipped ruleset, on the chain.** `palw-class preflight` judges a
/// real checkpoint at DAA 5,585 (every fence in force) and prints `register ok` at the widest context the gate admits; the
/// fixtures under `fixtures/g14/shipped` are those classes (shape-only lowering, synthetic root). The chain's processor, with the
/// rules it resolves at each height of the schedule, must agree with the gate everywhere and keep them from `palw_tir_v1` on.
#[tokio::test]
async fn g14_real_checkpoint_class_at_its_widest_context_across_the_shipped_schedule() {
    use super::t12_round_lane_e2e::t12_with_harness_cards_over;
    let cases = real_cases_in("shipped");
    if cases.is_empty() {
        eprintln!("[g14] no shipped-ruleset fixtures present");
        return;
    }
    let env = Env::over(t12_with_harness_cards_over(kaspa_consensus_core::config::params::palw_t12_shipped_params(), false));
    let (block, state) = env.chain.tip_state();
    let schedule = env.config.params.fence_schedule_v1();
    let mut heights: Vec<u64> = vec![0];
    for h in &schedule {
        heights.extend([h.saturating_sub(1), *h, h + 1]);
    }
    heights.sort_unstable();
    heights.dedup();
    let tir_at = env.config.params.palw_tir_v1_fence().map(|f| f.activation.daa_score()).expect("armed");
    let mut bad = Vec::new();
    for (i, c) in cases.iter().enumerate() {
        let mut flips = Vec::new();
        let mut last = None;
        for &daa in &heights {
            let j = judge(&env, &state, block, daa, &signed(&env, &state, &spec_of(c, 1 + i % 6, daa)));
            match (&j.preflight, &j.validate) {
                (Ok(()), Ok(())) if j.fold.as_ref().is_some_and(|f| f.is_ok()) && j.accepted => {}
                (Err(e), Err(why))
                    if why.contains(&e.to_string()) || (matches!(e, E::TirNeedsItsFence) && why.contains("not in force")) => {}
                _ => bad.push(format!("{} daa {daa}: {j:?}", c.name)),
            }
            if last != Some(j.preflight.is_ok()) {
                flips.push((daa, j.preflight.is_ok()));
                last = Some(j.preflight.is_ok());
            }
        }
        eprintln!("[g14] {} at context {}: verdict by height (daa, admitted) {flips:?}", c.name, c.class.layout.max_context);
        let at_end = judge(&env, &state, block, 5_585, &signed(&env, &state, &spec_of(c, 1 + i % 6, 5_585)));
        check(&format!("{} at DAA 5,585", c.name), &at_end, Want::Accept).unwrap();
        assert!(5_585 >= tir_at);
    }
    assert!(bad.is_empty(), "the gate and the chain disagree:\n{}", bad.join("\n"));
}

// ---- the whole release armed (compressed), real checkpoints at 32,783 positions, MINED -----------------------

/// **testnet-12 with every release flag day armed, in the release's order, at heights a test can reach**: the launch ruleset
/// with the DAA-750 list at 20, the second at 24, the capacity list at 28, `palw_tir_v1` at 32, `palw_tir_fence2` at 36 and the
/// int-11 list at 40 (its ρ steps at +95, +190, +285) — each fence through its own `set` so every mirror follows. After DAA 40 every
/// rule the shipped schedule has in force at DAA 5,585 governs admission (the corpus sweep above shows no verdict moves between
/// the real heights, so the compression moves none).
fn t12_release_compressed() -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    t12_release_compressed_over(None)
}

/// [`t12_release_compressed`] with — when `int13` is `Some(at)` — the int-13 flag day's list (`PALW_T12_INT13_FENCES_V1`: audit 1004, the
/// range twin, the model court window, V4 receipt redemption) at `at`, over the int-11 list at 40.
fn t12_release_compressed_over(int13: Option<u64>) -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    use super::t12_round_lane_e2e::t12_with_harness_cards_over;
    use kaspa_consensus_core::config::params::{
        PALW_T12_POST_LAUNCH_FENCES_V1, PALW_T12_POST_LAUNCH_FENCES_V2, PALW_T12_POST_LAUNCH_FENCES_V3, PALW_T12_TIR_FENCE2_FENCES_V1,
        PALW_T12_TIR_FLAG_DAY_FENCES_V1, palw_t12_arm_int11_flag_day_at_v1, palw_t12_arm_int13_flag_day_at_v1, palw_t12_launch_params_v1,
    };
    let mut params = palw_t12_launch_params_v1();
    for (list, at) in [
        (PALW_T12_POST_LAUNCH_FENCES_V1, 20),
        (PALW_T12_POST_LAUNCH_FENCES_V2, 24),
        (PALW_T12_POST_LAUNCH_FENCES_V3, 28),
        (PALW_T12_TIR_FLAG_DAY_FENCES_V1, 32),
        (PALW_T12_TIR_FENCE2_FENCES_V1, 36),
    ] {
        for fence in list {
            (fence.set)(&mut params, Some(ForkActivation::new(at)));
        }
    }
    palw_t12_arm_int11_flag_day_at_v1(&mut params, Some(40));
    if let Some(at) = int13 {
        palw_t12_arm_int13_flag_day_at_v1(&mut params, Some(at));
    }
    params.validate_palw_v2().expect("the whole release, compressed, is a runnable ruleset");
    t12_with_harness_cards_over(params, false)
}

/// Heartbeats until the sink's DAA is at least `daa`.
async fn warm_to(env: &mut Env, daa: u64) {
    let ttpb = env.config.params.target_time_per_block();
    while env.chain.daa_of(env.chain.sink()) < daa {
        env.chain.heartbeat(ttpb, Vec::new()).await;
    }
}

/// **The four real checkpoints at 32,783 positions, through the whole pipeline under the whole release.** The classes are the ones
/// the SDK's preflight registers under `palw_t12_shipped_params()` at DAA 5,585; here the chain is mined past the compressed
/// release's last IR fence and each class is carried by the node's own template, folded, persisted and read back.
#[tokio::test]
async fn g14_real_checkpoints_at_32783_positions_mined_under_the_whole_release() {
    let cases = real_cases_in("shipped");
    if cases.is_empty() {
        eprintln!("[g14] no shipped-ruleset fixtures present");
        return;
    }
    for (i, c) in cases.iter().enumerate() {
        let mut env = Env::over(t12_release_compressed());
        warm_to(&mut env, 41).await;
        let (block, state) = env.chain.tip_state();
        let daa = env.chain.daa_of(block);
        let j = judge(&env, &state, block, daa, &signed(&env, &state, &spec_of(c, 1 + i % 6, daa)));
        eprintln!(
            "[g14] {:<34} context {:>6} program {:>6} B at DAA {daa}: gate {:?}, chain_accepted {}",
            c.name,
            c.class.layout.max_context,
            c.class.program.len(),
            j.preflight,
            j.accepted
        );
        check(&c.name, &j, Want::Accept).unwrap();
        // The whole pipeline (mempool, template, fold, persisted tip, ConsensusApi reads, the activation flip) on this ruleset.
        mined_end_to_end_on(env, c, 1 + i % 6).await;
    }
    // And a second node replaying one such chain reaches the same roots (the follower is built on the same compressed ruleset).
    let mut env = Env::over(t12_release_compressed());
    warm_to(&mut env, 41).await;
    let a = mine_registration_on(env, &cases[0], 3).await;
    let z = fresh_node(&a);
    for b in chain_blocks(&a.env.chain, a.env.chain.sink()) {
        arrive(&z, b, "the release chain's block").await;
    }
    assert_eq!(z.sink(), a.env.chain.sink());
    assert_eq!(z.tip_state().1.state_root(), a.env.chain.tip_state().1.state_root(), "the replaying node's root");
    assert_eq!(z.tip_state().1.tir_class_v1(&a.class_id), Some(&a.record));
}

/// **The int-13 flag day's `palw_model_court_window`, on the real node path** (the coordinator's brief of 2026-10-08: it is armed at DAA 9,000
/// with the other tier-1 fences, and no processor test had ever registered a class under it). A class that needs a dissection (fused
/// attention) registered PAST the fence commits its own finite window — the node derives it (`palw_class_court_windows_for_objects`) and
/// the fold stores it — and on testnet-12's held clock that is the network window; registered BELOW the fence, on the same armed ruleset,
/// it commits none (the table is empty below the fence); on the ruleset with the list dormant it commits none at any height. A second node
/// replaying the chain folds the same root, window and all.
#[tokio::test]
async fn g14_a_dissected_class_registered_past_the_int13_court_window_commits_the_network_window() {
    const FENCE: u64 = 60;
    kaspa_core::log::try_init_logger("warn");
    let c = case("sliding-global");

    // Below the fence on the armed ruleset: registered, no window row.
    let mut env = Env::over(t12_release_compressed_over(Some(FENCE)));
    warm_to(&mut env, 41).await;
    let below = mine_registration_on(env, &c, 2).await;
    let (tip, state) = below.env.chain.tip_state();
    assert!(below.env.chain.daa_of(tip) < FENCE, "the control is registered below the fence");
    assert!(state.class(&below.class_id).is_some(), "the class is registered");
    assert_eq!(state.class_model_court_window_v1(&below.class_id), None, "registered below the fence: the network window, no row");

    // Past the fence: the same class commits its window.
    let mut env = Env::over(t12_release_compressed_over(Some(FENCE)));
    warm_to(&mut env, FENCE + 1).await;
    let past = mine_registration_on(env, &c, 3).await;
    let (tip, state) = past.env.chain.tip_state();
    assert!(past.env.chain.daa_of(tip) > FENCE, "registered past the fence");
    let network = past.env.bundle.state.window_court();
    let window = state.class_model_court_window_v1(&past.class_id).expect("a dissected class registered past the fence commits its window");
    assert!(window >= network, "never below the network window ({window} < {network})");
    assert_eq!(window, network, "on the held clock the derived window IS the network window for this class");
    assert_eq!(state.tir_class_v1(&past.class_id), Some(&past.record), "the class row is what the gate derived");
    eprintln!("[g14] past palw_model_court_window ({FENCE}): {} commits window {window} (network {network})", c.name);

    // A second node replays the chain to the same root, window included.
    let z = fresh_node(&past);
    for b in chain_blocks(&past.env.chain, past.env.chain.sink()) {
        arrive(&z, b, "the armed chain's block").await;
    }
    assert_eq!(z.sink(), past.env.chain.sink());
    assert_eq!(z.tip_state().1.state_root(), state.state_root(), "the replaying node's root");
    assert_eq!(z.tip_state().1.class_model_court_window_v1(&past.class_id), Some(window));

    // The ruleset with the list dormant: the same registration past the same height commits no window.
    let mut env = Env::over(t12_release_compressed_over(None));
    warm_to(&mut env, FENCE + 1).await;
    let dormant = mine_registration_on(env, &c, 3).await;
    let (_, dstate) = dormant.env.chain.tip_state();
    assert!(dstate.class(&dormant.class_id).is_some());
    assert_eq!(dstate.class_model_court_window_v1(&dormant.class_id), None, "dormant: no window row at any height");
}
