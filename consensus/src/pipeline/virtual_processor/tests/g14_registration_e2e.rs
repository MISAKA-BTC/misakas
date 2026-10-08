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
use super::t12_round_lane_e2e::{T12Chain, card_payout_spk, sign_spend, t12_genesis_chain, t12_with_harness_cards};
use crate::consensus::test_consensus::TestConsensus;
use kaspa_consensus_core::BlockHash;
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
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../consensus-vectors/tir-v1/programs");
    let mut files: Vec<PathBuf> =
        std::fs::read_dir(dir).expect("vectors").map(|e| e.unwrap().path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
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
                    max_context: 64,
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
            class.layout = layout_of(&class, 64);
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
        let (config, bundle, premine, floats) = t12_tir_config(tir_at);
        let chain = t12_genesis_chain(&config, &bundle, &premine, &floats);
        let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
            config.params.net.to_string().as_bytes(),
            Some(config.params.genesis.hash),
        );
        Env { chain, config, bundle, floats, domain }
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
            edit_raw(&env, &state, &s, |o| {
                reg!(o, { pwu_rule } => *pwu_rule = PalwPwuRuleV2::DerivedV1 { pwu_per_inference: counted + 1 })
            }),
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
        (
            "signature: none (unsigned)",
            build(&env, &state, &s),
            Want::StatefulDrop("is not signed by the bond it names"),
        ),
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
    check("duplicate: the same class by another registrant", &judge(&env, &after, block, 0, &by_another), Want::StatefulDrop("already"))
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
