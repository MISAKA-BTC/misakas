//! The RFC-0012 rig supplies its test-only mature work fact. Fork choice,
//! executed headers, settlement generations and withdrawal readback are native.
//! VSS outcomes, allocated resources and the schedule are fixture inputs.
//! Each native database resolves the closure from the same accepted close block.
use super::*;
#[path = "tb1_qf1_embedding_fixture.rs"]
mod embedding;
#[path = "tb1_exec_input.rs"]
mod exec_input;
#[path = "tb1_qf1_prediction_fixture.rs"]
mod prediction;
use kaspa_rpc_core::RpcRawBlock;
use misaka_palw_remote::tb1_exposure::exec_input::{ExecInputBatch, QualifiedExecPanel, REGISTRATION_DOMAIN, SignedExecInput};
use misaka_palw_remote::tb1_exposure::publication::{PublicationLimits, PublishedRelease, batch_commitment};
use misaka_palw_remote::tb1_exposure::qf1::Qf1Use;
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::{Read, Write},
    os::{
        fd::{AsFd, OwnedFd},
        unix::net::UnixStream,
    },
    path::Path,
    process::{Child, Command, Stdio},
    time::Duration,
};

// Format each record before writing it. Several processes share stderr.
macro_rules! peer_log {
    ($($arg:tt)*) => {{
        let mut line = format!($($arg)*);
        line.push('\n');
        std::io::stderr().lock().write_all(line.as_bytes()).unwrap();
    }};
}

#[path = "tb1_model_audit.rs"]
mod model_audit;

#[path = "tb1_moe2_offers.rs"]
mod offers;

#[path = "tb1_stable_panel.rs"]
mod stable_panel;

const QF1_TEST_TOKENIZER: &str =
    "10ed14bc48937bd836b309447040a8d43d38f103fde39682f7f9f2c4cbb6b4b4666658ab71ed3d90de7c6e2e00cce21c01863b4403ae41dab361b099fdfe2ec5";
const READER_DIR: &str = "MISAKA_PRIVATE_TB1_FREEZE_READER";
const READER_SLOT: &str = "MISAKA_PRIVATE_TB1_FREEZE_SLOT";

use kaspa_consensus_core::palw_tb1::{
    Tb1Field,
    freeze::{Tb1MiningClosure, Tb1MiningPurpose, Tb1MiningRound},
    qualification::{Tb1DealerEnd, Tb1PreparationCut, Tb1Qualification, Tb1Ready, Tb1Roster, Tb1VssEnd},
    releases::{Tb1HolderRelease, Tb1ReleaseContext},
    signatures::{READY_CONTEXT, RELEASE_CONTEXT, ready_message},
};
use libcrux_ml_dsa::ml_dsa_65;
use misaka_palw_remote::tb1_exposure::{Tb1ExposureJournal, Tb1HolderFreeze};

const LIMITS: PublicationLimits = PublicationLimits { records: 4, message_bytes: 2048, matrix_cells: 100 };

#[derive(Serialize, Deserialize)]
struct ReleaseRecord {
    holder: usize,
    shares: Vec<(usize, Vec<u8>)>,
    signature: Vec<u8>,
}

struct DecodedRecord {
    holder: usize,
    shares: Vec<(usize, Tb1Field)>,
    signature: ml_dsa_65::MLDSA65Signature,
}

impl ReleaseRecord {
    fn decode(&self) -> DecodedRecord {
        DecodedRecord {
            holder: self.holder,
            shares: self.shares.iter().map(|(d, v)| (*d, Tb1Field::from_bytes(v.as_slice().try_into().unwrap()).unwrap())).collect(),
            signature: ml_dsa_65::MLDSA65Signature::new(self.signature.as_slice().try_into().unwrap()),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct Publication {
    batch: RpcRawBlock,
    certificate: RpcRawBlock,
    rows: Vec<ReleaseRecord>,
}

// Private fixture transport between live instances of the same executable.
// A length-delimited full-duplex channel also separates data from test output.
fn send<T: Serialize>(socket: &mut UnixStream, value: &T) {
    let bytes = serde_json::to_vec(value).unwrap();
    socket.write_all(&u32::try_from(bytes.len()).unwrap().to_be_bytes()).unwrap();
    socket.write_all(&bytes).unwrap();
}

fn receive<T: serde::de::DeserializeOwned>(socket: &mut UnixStream) -> T {
    let mut size = [0; 4];
    socket.read_exact(&mut size).unwrap();
    let size = u32::from_be_bytes(size) as usize;
    assert!(size < 4_000_000);
    let mut bytes = vec![0; size];
    socket.read_exact(&mut bytes).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn child_reader(dir: &Path, slot: usize, stable: bool, registered: bool) -> (Child, UnixStream) {
    let (parent, child_input) = UnixStream::pair().unwrap();
    let timeout = if std::env::var_os("MISAKA_PRIVATE_QF1_FULL").is_some() { 3600 } else { 60 };
    for s in [&parent, &child_input] {
        s.set_read_timeout(Some(Duration::from_secs(timeout))).unwrap();
        s.set_write_timeout(Some(Duration::from_secs(timeout))).unwrap();
    }
    let mut command = Command::new(std::env::current_exe().unwrap());
    if registered {
        command.args(["private_registered_tb1_pending_freeze", "--ignored"]);
    } else if stable {
        command.args(["private_tb1_stable_panel", "--ignored"]);
    } else {
        command.arg("native_withdrawal_halts_an_exposed_holder_while_its_block_is_still_selected");
    }
    let child = command
        .args(["--test-threads=1", "--nocapture"])
        .env(READER_DIR, dir)
        .env(READER_SLOT, slot.to_string())
        .stdin(Stdio::from(OwnedFd::from(child_input)))
        .spawn()
        .unwrap();
    (child, parent)
}

async fn local_carrier<const N: usize>(
    node: &mut Rig,
    parent: &mut Option<UnixStream>,
    peers: &mut [(Child, UnixStream)],
    card: usize,
    marker: [u8; N],
    label: &str,
) -> Block {
    if let Some(socket) = parent {
        let block = Block::try_from(receive::<RpcRawBlock>(socket)).unwrap();
        stable_panel::arrive(node, block.clone(), label).await;
        block
    } else {
        let miner = MinerData::new(card_payout_spk(card), marker.to_vec());
        let template = stable_panel::carrier(node, card, miner);
        let block = node.take(template, label).await;
        for (_, s) in peers {
            send(s, &RpcRawBlock::from(&block));
        }
        block
    }
}

fn views<'a>(rows: &'a [DecodedRecord], context: Tb1ReleaseContext<'a>) -> Vec<PublishedRelease<'a>> {
    rows.iter()
        .map(|r| PublishedRelease {
            row: Tb1HolderRelease { context, holder_slot: r.holder, shares: &r.shares },
            signature: &r.signature,
        })
        .collect()
}

fn field(v: usize) -> Tb1Field {
    let mut bytes = [0; 66];
    bytes[65] = v as u8;
    Tb1Field::from_bytes(&bytes).unwrap()
}

fn qualified<'a>(
    round_ref: &'a [u8],
    roster: Tb1Roster<'a>,
    keys: &[ml_dsa_65::MLDSA65KeyPair],
    dealers: &'a [usize],
    session: u8,
    transcripts: Option<&[Hash64; 4]>,
) -> Tb1Qualification<'a> {
    qualified_recording_ready(round_ref, roster, keys, dealers, session, transcripts, |_| {})
}

fn qualified_recording_ready<'a>(
    round_ref: &'a [u8],
    roster: Tb1Roster<'a>,
    keys: &[ml_dsa_65::MLDSA65KeyPair],
    dealers: &'a [usize],
    session: u8,
    transcripts: Option<&[Hash64; 4]>,
    mut record: impl FnMut(&[u8]),
) -> Tb1Qualification<'a> {
    qualification_from_ready(round_ref, roster, dealers, session, transcripts, |_, r| {
        let sig = ml_dsa_65::sign(&keys[r.holder_slot].signing_key, &ready_message(r, 2048).unwrap(), READY_CONTEXT, [9; 32]).unwrap();
        record(sig.as_slice());
        sig.as_slice().to_vec()
    })
    .unwrap()
}

fn qualification_from_ready<'a>(
    round_ref: &'a [u8],
    roster: Tb1Roster<'a>,
    dealers: &'a [usize],
    session: u8,
    transcripts: Option<&[Hash64; 4]>,
    mut signature: impl FnMut(usize, &Tb1Ready<'_>) -> Vec<u8>,
) -> Result<Tb1Qualification<'a>, kaspa_consensus_core::palw_tb1::Tb1PolynomialError> {
    let ends: Vec<_> = (0..4)
        .map(|d| {
            Some(Tb1DealerEnd {
                end: Tb1VssEnd {
                    end_anchor: [session; 64],
                    transcript_root: transcripts.map_or([session + d; 64], |t| t[d as usize].as_bytes()),
                },
                position: 1,
            })
        })
        .collect();
    let mut ready = Vec::new();
    for &dealer in dealers {
        for holder_slot in 0..3 {
            ready.push(Tb1Ready {
                round_ref,
                dealer_slot: dealer,
                holder_slot,
                position: 2,
                end: ends[dealer].unwrap().end,
                signature: &[],
            });
        }
    }
    let signatures: Vec<_> = ready.iter().enumerate().map(|(i, r)| signature(i, r)).collect();
    for (r, sig) in ready.iter_mut().zip(&signatures) {
        r.signature = sig.as_slice();
    }
    let cut = Tb1PreparationCut { round_ref, deadline_position: 3, observed_through: 3, dealer_ends: &ends, ready: &ready };
    Tb1Qualification::resolve(
        roster,
        round_ref,
        [session; 64], // reserved resources reference
        dealers,
        &cut,
        9,
        2048,
    )
}

#[tokio::test]
async fn native_withdrawal_halts_an_exposed_holder_while_its_block_is_still_selected() {
    run(false, false).await;
}

#[tokio::test]
#[ignore = "requires the RFC-0066 tokenizer asset in MISAKA_PRIVATE_QF1_TOKENIZER"]
async fn private_tb1_stable_panel() {
    assert!(std::env::var_os("MISAKA_PRIVATE_QF1_TOKENIZER").is_some());
    if std::env::var_os("MISAKA_PRIVATE_QF1_ATTENTION").is_some() {
        assert!(std::env::var_os("MISAKA_PRIVATE_QF1_EMBEDDING").is_some(), "the attention check needs authenticated input rows");
    }
    run(true, false).await;
}

#[tokio::test]
#[ignore = "validates ordinary verifier maturity and generates genuine certified history"]
async fn private_tb1_certified_floor_history() {
    use super::super::{g14_registration_e2e::private_moe2::arm_private_registration, t12_round_lane_e2e::certified_floor_history};
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
    let (config, _, premine, floats) = parts_custom(Some(FENCE), false, |p| arm_private_registration(p, true));
    let PalwConsensusMode::ConsensusV2(bundle) = &config.params.palw_consensus_mode else { unreachable!() };
    let mut node = Rig::new(&(config.clone(), bundle.clone(), premine, floats.clone()), 0x12_c500_0000);
    stable_panel::arm(&mut node);
    certified_floor_history(&mut node.chain, &mut floats[0].clone()).await;
}

#[test]
#[ignore = "requires the QF1 parent and scale section and temporary space for the portable artifact"]
fn private_qf1_work_key_matches_composite_and_portable_artifact() {
    use misaka_palw_sdk::qf1_local::residual_tiles::Candidate;
    use misaka_palw_tir_exec::node::TirArtifactV1;
    use std::{fs::File, io::BufWriter};

    let parent = std::env::var_os("MISAKA_PRIVATE_QF1_FULL").expect("QF1 parent artifact");
    let section = std::env::var_os("MISAKA_PRIVATE_QF1_TILES").expect("QF1 registered scale section");
    let parent = TirArtifactV1::open(Path::new(&parent)).unwrap();
    let recipe = Candidate::over(&parent).unwrap();
    let split_artifact = recipe.open(&parent, Path::new(&section)).unwrap();
    let split = recipe.bind(&split_artifact).unwrap();

    let dir = kaspa_database::utils::get_kaspa_tempdir();
    let path = dir.path().join("portable.palwtir");
    recipe.write_full(&split_artifact, BufWriter::new(File::create(&path).unwrap())).unwrap();
    let full_artifact = TirArtifactV1::open(&path).unwrap();
    let full = recipe.bind_full(&full_artifact).unwrap();
    assert_ne!(split.identity().class, full.identity().class);

    let prompt = [151644, 8948, 198, 1, 2, 3, 151645];
    let work = split.computational_work_key(&prompt, 2).unwrap();
    assert_eq!(full.computational_work_key(&prompt, 2).unwrap(), work);
    assert_ne!(full.computational_work_key(&prompt, 3).unwrap(), work);
    println!("QF1 composite and portable numerical work key {work}");
}

#[tokio::test]
#[ignore = "requires the full registered QF1 artifact and tokenizer"]
async fn private_registered_tb1_pending_freeze() {
    assert!(std::env::var_os("MISAKA_PRIVATE_QF1_FULL").is_some());
    run(true, true).await;
}

fn inconsistent_readout_fixture() -> bool {
    std::env::var_os("MISAKA_PRIVATE_QF1_INCONSISTENT_READOUT").is_some()
}

async fn run(stable: bool, registered: bool) {
    use super::super::g14_registration_e2e::{private_moe2::arm_private_registration, tb1_registration_prefix};
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
    let child_dir = std::env::var_os(READER_DIR).map(std::path::PathBuf::from);
    let dir = kaspa_database::utils::get_kaspa_tempdir();
    let full = std::env::var_os("MISAKA_PRIVATE_QF1_FULL");
    let tokenizer = std::env::var_os("MISAKA_PRIVATE_QF1_TOKENIZER");
    assert!(full.is_none() || (stable && tokenizer.is_some()), "the full class uses the stable round and pinned tokenizer");
    assert!(!inconsistent_readout_fixture() || (stable && !registered && tokenizer.is_some()));
    let held = full.as_ref().map(|p| misaka_palw_tir_exec::node::TirArtifactV1::open(Path::new(p)).unwrap());
    let tiles = std::env::var_os("MISAKA_PRIVATE_QF1_TILES").map(|path| {
        let parent = held.as_ref().expect("QF1 tile scales require their registered parent");
        let recipe = misaka_palw_sdk::qf1_local::residual_tiles::Candidate::over(parent).unwrap();
        let asset = recipe.open(parent, Path::new(&path)).unwrap();
        (recipe, asset)
    });
    let (recipe, held) = match tiles {
        Some((recipe, asset)) => (Some(recipe), Some(asset)),
        None => (None, held),
    };
    let exercise = tokenizer.as_ref().map(|_| match held {
        Some(a) => {
            let norm = recipe.as_ref().map_or_else(|| misaka_palw_sdk::qf1_local::program().unwrap().final_norm, |r| r.final_norm());
            (a, norm)
        }
        None => prediction::artifact(&dir.path().join("schedule.palwtir")),
    });
    let instance = exercise.as_ref().map(|(a, norm)| match (&recipe, &full) {
        (Some(candidate), _) => candidate.bind_full(a).unwrap(),
        (_, Some(_)) => misaka_palw_sdk::qf1_local::bind(a).unwrap(),
        _ => prediction::bind(a, *norm),
    });
    let parts = if registered {
        let (config, _, premine, floats) = parts_custom(Some(FENCE), false, |p| arm_private_registration(p, true));
        let PalwConsensusMode::ConsensusV2(bundle) = &config.params.palw_consensus_mode else { unreachable!() };
        let bundle = bundle.clone();
        (config, bundle, premine, floats)
    } else {
        parts(Some(FENCE))
    };
    let mut node = Rig::new(&parts, 0x12_c500_0000);
    let genesis = node.chain.sink();
    if stable {
        stable_panel::arm(&mut node);
    }
    let mut registered_quote = None;
    let mut quote_anchor_state = None;
    let registration = if registered {
        let class = exercise.as_ref().unwrap().0.class().unwrap();
        let inventory = instance.as_ref().unwrap().identity().inventory;
        let class_id = class.class_id(&inventory);
        let filename = "held-registration.json";
        let prefix: Vec<Block> = if let Some(path) = &child_dir {
            let raw: Vec<RpcRawBlock> = serde_json::from_reader(File::open(path.join(filename)).unwrap()).unwrap();
            raw.into_iter().map(|r| Block::try_from(r).unwrap()).collect()
        } else {
            let blocks = tb1_registration_prefix(&parts, class.clone(), inventory).await;
            let raw: Vec<_> = blocks.iter().map(RpcRawBlock::from).collect();
            serde_json::to_writer(File::create_new(dir.path().join(filename)).unwrap(), &raw).unwrap();
            blocks
        };
        assert!(node.api().palw_tir_class_record_v1(class_id).is_none());
        for block in prefix {
            stable_panel::arrive(&mut node, block, "native model registration prefix").await;
        }
        let (fold, state) = node.chain.tip_state();
        assert_eq!(fold, node.chain.sink());
        let registered = state.class(&class_id).expect("model registration on the EXEC chain");
        assert_eq!(registered.artifact_root, inventory);
        assert!(state.model_market(&class_id).is_some());
        let native = node.api().palw_tir_class_record_v1(class_id).unwrap();
        assert_eq!(
            (native.graph_ir_root, native.layout_digest, native.tokenizer_id),
            (class.graph_ir_root(), class.layout_digest(), class.tokenizer_id)
        );
        peer_log!(
            "QF1 registered pid={} block={} class={} artifact={} state={} daa={} blue={}",
            std::process::id(),
            fold,
            class_id,
            inventory,
            state.state_root(),
            node.chain.daa_of(fold),
            node.api().get_header(fold).unwrap().blue_score
        );
        registered_quote = Some(super::super::g14_registration_e2e::private_moe2::moe2_reservation::registration_quote(
            &state,
            class_id,
            node.chain.bonds[1],
            node.bundle.state.bond_budget().unwrap().policy.digest(),
        ));
        quote_anchor_state = Some(state);
        Some((fold, class_id))
    } else {
        None
    };
    let prefix_tip = node.api().get_header(node.chain.sink()).unwrap();
    let (daa_offset, blue_offset) = if registration.is_some() { (prefix_tip.daa_score, prefix_tip.blue_score) } else { (0, 0) };
    let template = node.beat();
    let keys: Vec<_> = (1..=4).map(|i| ml_dsa_65::generate_key_pair([i; 32])).collect();
    let public: Vec<_> = keys.iter().map(|kp| kp.verification_key.clone()).collect();
    let coordinates: Vec<_> = (1..=4).map(field).collect();
    let roster = Tb1Roster { coordinates: &coordinates, holder_keys: &public, f: 1 };
    let round = Tb1MiningRound {
        chain_id: node.domain.as_bytes(),
        roster_root: roster.root().unwrap(),
        role_epoch: 1,
        logical_round: 2,
        purpose: Tb1MiningPurpose::Exec,
        schedule_id: [9; 64],
        close_blue_score: template.header.blue_score + 1,
        release_from_daa: if stable { daa_offset + 4 } else { template.header.daa_score + 1 },
        deadline_exclusive_daa: if stable { daa_offset + 32 } else { template.header.daa_score + 11 },
    };
    let round_ref = round.id();
    let round_ref = round_ref.as_slice();
    let dealers = [0, 1, 2];
    let panel = Tb1MiningRound {
        purpose: Tb1MiningPurpose::Panel,
        logical_round: 3,
        // Seven selected blocks take EXEC close to its certificate; the
        // next two carry the signed Original and the private PANEL close.
        close_blue_score: if stable { blue_offset + 25 } else { round.close_blue_score + 9 },
        release_from_daa: if stable { daa_offset + 26 } else { 20 },
        deadline_exclusive_daa: if stable { daa_offset + 50 } else { 30 },
        ..round
    };
    let panel_id = panel.id();
    let q = qualified(round_ref, roster, &keys, &dealers, 7, None);
    let slot = if child_dir.is_some() { std::env::var(READER_SLOT).unwrap().parse::<usize>().unwrap() } else { 0 };
    let setup = stable.then(|| stable_panel::provision(dir.path(), child_dir.as_deref(), slot, &panel_id));
    let mut panel_ready = Vec::new();
    let panel_q = qualified_recording_ready(&panel_id, roster, &keys, &dealers, 29, setup.as_ref().map(|p| &p.roots), |sig| {
        panel_ready.push(sig.to_vec())
    });
    assert!(QualifiedExecPanel::new(round, &q, panel, &q).is_err());
    assert!(QualifiedExecPanel::new(round, &panel_q, panel, &q).is_err());
    let rounds = QualifiedExecPanel::new(round, &q, panel, &panel_q).unwrap();
    let preparation = if let Some(path) = &child_dir {
        let raw: RpcRawBlock = serde_json::from_reader(File::open(path.join("preparation.json")).unwrap()).unwrap();
        let block = Block::try_from(raw).unwrap();
        stable_panel::arrive(&mut node, block.clone(), "recorded EXEC/PANEL preparation").await;
        block
    } else {
        let mut miner = new_miner_data();
        miner.extra_data = rounds.commitment().to_vec();
        let t = stable_panel::heartbeat(&mut node, miner);
        let block = node.take(t, "joint EXEC/PANEL qualification").await;
        serde_json::to_writer(File::create_new(dir.path().join("preparation.json")).unwrap(), &RpcRawBlock::from(&block)).unwrap();
        block
    };
    let parent = preparation.hash();
    assert_eq!(node.chain.sink(), parent);
    let mut terms = exec_input::terms(parent);
    if stable {
        terms.deadline_exclusive_daa = daa_offset + 60;
    }
    let owner = ml_dsa_65::generate_key_pair([82; 32]);
    let key = terms.registration_key(parent, &rounds, &owner.verification_key, 16).unwrap();
    let owner_sig = if let Some(path) = std::env::var_os(READER_DIR) {
        let bytes = std::fs::read(Path::new(&path).join("input.sig")).unwrap();
        ml_dsa_65::MLDSA65Signature::new(bytes.as_slice().try_into().unwrap())
    } else {
        let sig = ml_dsa_65::sign(&owner.signing_key, &key, REGISTRATION_DOMAIN, [83; 32]).unwrap();
        std::fs::write(dir.path().join("input.sig"), sig.as_slice()).unwrap();
        sig
    };
    let entry = SignedExecInput {
        terms: &terms,
        owner: &owner.verification_key,
        signature: &owner_sig,
        slot: 7,
        funding_ref: [23; 64],
        quote_consent: None,
    };
    let changed = misaka_palw_remote::tb1_exposure::exec_input::ExecInputTerms { job_tokens: &[42, 108, 18], ..terms };
    assert!(
        ExecInputBatch::from_signed(node.api().as_ref(), parent, &rounds, &[SignedExecInput { terms: &changed, ..entry }], 4, 16)
            .is_err()
    );
    assert!(
        ExecInputBatch::from_signed(
            node.api().as_ref(),
            parent,
            &rounds,
            &[SignedExecInput { slot: 8, ..entry }, SignedExecInput { ..entry }],
            4,
            16
        )
        .is_err()
    );
    assert!(ExecInputBatch::from_signed(node.api().as_ref(), genesis, &rounds, &[], 4, 16).is_err());
    let changed_panel_q = qualified(&panel_id, roster, &keys, &dealers, 30, None);
    let other_rounds = QualifiedExecPanel::new(round, &q, panel, &changed_panel_q).unwrap();
    assert!(ExecInputBatch::from_signed(node.api().as_ref(), parent, &other_rounds, &[], 4, 16).is_err());
    let early_terms = misaka_palw_remote::tb1_exposure::exec_input::ExecInputTerms { registration_anchor: genesis, ..terms };
    let early_key = early_terms.registration_key(parent, &rounds, &owner.verification_key, 16).unwrap();
    let early_sig = ml_dsa_65::sign(&owner.signing_key, &early_key, REGISTRATION_DOMAIN, [83; 32]).unwrap();
    assert!(
        ExecInputBatch::from_signed(
            node.api().as_ref(),
            parent,
            &rounds,
            &[SignedExecInput { terms: &early_terms, signature: &early_sig, ..entry }],
            4,
            16
        )
        .is_err()
    );
    let other_key = terms.registration_key(parent, &other_rounds, &owner.verification_key, 16).unwrap();
    assert_ne!(other_key, key);
    let wrong_pair_sig = ml_dsa_65::sign(&owner.signing_key, &other_key, REGISTRATION_DOMAIN, [83; 32]).unwrap();
    assert!(
        ExecInputBatch::from_signed(
            node.api().as_ref(),
            parent,
            &rounds,
            &[SignedExecInput { signature: &wrong_pair_sig, ..entry }],
            4,
            16
        )
        .is_err()
    );
    if let Some((block, class_id)) = registration {
        assert_eq!(instance.as_ref().unwrap().identity().class, class_id);
        assert!(node.api().is_chain_ancestor_of(block, parent).unwrap());
        let bound = instance.as_ref().unwrap();
        // The folding block's header still commits its parent, before the class was installed.
        assert!(misaka_palw_sdk::qf1_local::registration_at(node.api().as_ref(), block, bound).is_err());
        let root = misaka_palw_sdk::qf1_local::registration_at(node.api().as_ref(), parent, bound).unwrap();
        peer_log!(
            "QF1 registered EXEC pid={} registration={} preparation={} parent_state={}",
            std::process::id(),
            block,
            parent,
            root
        );
    }
    let qf1 = tokenizer.as_ref().map(|path| {
        use misaka_palw_remote::tb1_exposure::qf1::Qf1Prompt;
        let bytes = std::fs::read(path).unwrap();
        let expected: Hash64 = QF1_TEST_TOKENIZER.parse().unwrap();
        let job = std::env::var("MISAKA_PRIVATE_QF1_JOB").unwrap_or_else(|error| match error {
            std::env::VarError::NotPresent => "2 + 2?".to_owned(),
            _ => panic!("QF1 job must be UTF-8"),
        });
        Qf1Prompt::from_registered_tokenizer(
            &bytes,
            expected,
            instance.as_ref().unwrap().identity().descriptor,
            &job,
            16_000_000,
            128,
        )
        .unwrap()
    });
    let qf1_terms = qf1.as_ref().map(|prompt| misaka_palw_remote::tb1_exposure::exec_input::ExecInputTerms {
        qf1: Some(Qf1Use::LocalInference(prompt)),
        moe2_quote: registered_quote,
        tokenizer: QF1_TEST_TOKENIZER.parse().unwrap(),
        job_tokens: prompt.job_tokens(),
        vocab: 151936,
        max_positions: 8192,
        model_root: instance.as_ref().unwrap().identity().inventory,
        class_id: instance.as_ref().unwrap().identity().class,
        // A fresh isolated D=1 profile fixes its difficulty before the EXEC close.
        target: if stable {
            let mut t = [0; 33];
            t[0] = 1;
            t
        } else {
            terms.target
        },
        decode_tokens: if recipe.is_some() { 16 } else { 3 },
        max_leaves: if full.is_some() { 32_000_000 } else { terms.max_leaves },
        ..terms
    });
    if let (Some(prompt), Some(terms)) = (&qf1, &qf1_terms) {
        let mining = misaka_palw_remote::tb1_exposure::exec_input::ExecInputTerms { qf1: Some(Qf1Use::NewMining(prompt)), ..*terms };
        assert!(
            mining.registration_key(parent, &rounds, &owner.verification_key, 16).unwrap_err().to_string().contains("REVIEW_HOLD")
        );
    }
    // A separate fresh job uses the same model and funder. Its owner and
    // funder sign a later economic horizon *before* this common close.
    let long_terms = registered_quote.map(|quote| {
        let cert = node.bundle.state.rfc0018_v1().unwrap().certification;
        let deadline = panel.deadline_exclusive_daa + cert.verifier_maturity_daa + cert.prosecution_window_daa + cert.lifetime_daa();
        misaka_palw_remote::tb1_exposure::exec_input::ExecInputTerms {
            nonce: [0x36; 32],
            deadline_exclusive_daa: deadline + 1,
            moe2_quote: Some(kaspa_consensus_core::palw_model_market_v1::moe2::settlement::Moe2PreDrawQuote { deadline, ..quote }),
            ..*qf1_terms.as_ref().unwrap()
        }
    });
    let mut entries = vec![entry];
    let mut signatures = Vec::new();
    for (slot, terms) in [(9, qf1_terms.as_ref()), (10, long_terms.as_ref())] {
        let Some(terms) = terms else { continue };
        let key = terms.registration_key(parent, &rounds, &owner.verification_key, 16).unwrap();
        let sig = offers::Signatures::read_or_sign(child_dir.as_deref(), dir.path(), &owner, key, slot, terms.moe2_quote.is_some());
        signatures.push((slot, terms, sig));
    }
    for (slot, terms, sig) in &signatures {
        let entry = sig.entry(terms, &owner.verification_key, *slot);
        if let Some(quote) = terms.moe2_quote {
            exec_input::check_quote_substitutions(node.api().as_ref(), parent, &rounds, &entry, quote);
        }
        entries.push(entry);
    }
    let inputs = ExecInputBatch::from_signed(node.api().as_ref(), parent, &rounds, &entries, 4, 16).unwrap();
    let closure = Tb1MiningClosure { round, qual_root: q.root(), consumer_set_root: inputs.root(), policy_root: terms.policy };

    let freeze_id = closure.commitment(parent);
    peer_log!(
        "TB1 qualification pid={} preparation={} exec={} panel={} consumer={}",
        std::process::id(),
        parent,
        Hash64::from_bytes(q.root()),
        Hash64::from_bytes(panel_q.root()),
        Hash64::from_bytes(inputs.root())
    );
    if let Some((model_block, _)) = registration {
        let (prepared_tip, close_parent_state) = node.chain.tip_state();
        assert_eq!(prepared_tip, parent);
        let file = "pending-freeze-blocks.json";
        let (anchor, assignment) = if let Some(dir) = &child_dir {
            let raw: Vec<RpcRawBlock> = serde_json::from_reader(File::open(dir.join(file)).unwrap()).unwrap();
            let assignment = Block::try_from(raw[2].clone()).unwrap().hash();
            let mut blocks = raw.into_iter().map(|r| Block::try_from(r).unwrap());
            let close = blocks.next().unwrap();
            let anchor = close.hash();
            stable_panel::arrive(&mut node, close, "registered EXEC close").await;
            for block in blocks {
                stable_panel::arrive(&mut node, block, "registered release window").await;
            }
            (anchor, assignment)
        } else {
            let mut miner = new_miner_data();
            miner.extra_data = freeze_id.to_vec();
            let t = stable_panel::heartbeat(&mut node, miner);
            let close = node.take(t, "registered EXEC close").await;
            let mut blocks = vec![RpcRawBlock::from(&close)];
            let mut funding = parts.3[0].clone();
            blocks.extend(offers::assign(&mut node, registered_quote.unwrap(), &mut funding).await);
            let assignment = node.chain.sink();
            while node.chain.daa_of(node.chain.sink()) < round.release_from_daa {
                assert!(blocks.len() < 120, "native release DAA did not advance");
                blocks.push(RpcRawBlock::from(&stable_panel::next(&mut node, 0).await));
            }
            serde_json::to_writer(File::create_new(dir.path().join(file)).unwrap(), &blocks).unwrap();
            (close.hash(), assignment)
        };
        let before = snapshot_of(&node).unwrap();
        let daa = node.chain.daa_of(before.generation);
        assert!((round.release_from_daa..round.deadline_exclusive_daa).contains(&daa));
        assert_eq!(before.generation, node.chain.sink());
        let api = node.api().as_ref();
        assert!(before.finalized.is_none());
        let ready = readiness_of(&node);
        assert_eq!(ready.stop, Some(kaspa_consensus_core::palw_native_settlement_v1::SettlementStopV1::FrontierNotCovered));
        let freeze = Tb1HolderFreeze::resolve(api, anchor, &closure, &q).unwrap();
        let closed_quote = inputs.read_closed_moe2_quote(api, anchor, &closure, &q, 9).unwrap();
        let long = inputs.read_closed_moe2_quote(api, anchor, &closure, &q, 10).unwrap();
        assert_eq!(long.terms(), long_terms.as_ref().unwrap().moe2_quote.unwrap());
        let quote = closed_quote.terms();
        assert_eq!(Some(quote), registered_quote);
        assert!(inputs.read_closed_moe2_quote(api, anchor, &closure, &q, 7).is_err());
        assert!(inputs.read_closed_moe2_quote(api, parent, &closure, &q, 9).is_err());
        for row in &entries {
            let key = row.terms.registration_key(parent, &rounds, row.owner, 16).unwrap();
            peer_log!(
                "MOE2 allocation row pid={} rounds={} slot={} key={} reference={}",
                std::process::id(),
                Hash64::from_bytes(rounds.commitment()),
                row.slot,
                Hash64::from_bytes(key),
                Hash64::from_bytes(row.funding_ref)
            );
        }
        let original_state = quote_anchor_state.as_ref().unwrap();
        assert_eq!(original_state.state_root(), api.get_header(parent).unwrap().palw_state_root);
        let public_t12 = kaspa_consensus_core::config::params::palw_t12_shipped_params();
        let budget_preview = |config, state| inputs.preview_closed_moe2_budget(api, config, anchor, &closure, &q, state);
        assert!(budget_preview(&public_t12, &close_parent_state).is_err());
        assert!(budget_preview(&node.config.params, original_state).is_err());
        let full = budget_preview(&node.config.params, &close_parent_state).unwrap();
        assert_eq!(full.keys().copied().collect::<Vec<_>>(), [9, 10]);
        {
            use kaspa_consensus_core::{
                api::ConsensusApi, errors::consensus::ConsensusResult, header::Header, trusted::ExternalGhostdagData,
            };
            use std::sync::Arc;
            struct CandidateHistory<'a>(&'a dyn ConsensusApi);
            impl ConsensusApi for CandidateHistory<'_> {
                fn get_sink(&self) -> Hash64 {
                    panic!("candidate validation must not consult the live sink");
                }
                fn get_header(&self, block: Hash64) -> ConsensusResult<Arc<Header>> {
                    self.0.get_header(block)
                }
                fn get_ghostdag_data(&self, block: Hash64) -> ConsensusResult<ExternalGhostdagData> {
                    self.0.get_ghostdag_data(block)
                }
                fn is_chain_ancestor_of(&self, low: Hash64, high: Hash64) -> ConsensusResult<bool> {
                    self.0.is_chain_ancestor_of(low, high)
                }
                fn palw_coinbase_commits_to(&self, chain: [u8; 64], block: Hash64, id: &[u8; 64]) -> ConsensusResult<bool> {
                    self.0.palw_coinbase_commits_to(chain, block, id)
                }
            }
            let history = CandidateHistory(api);
            let batch_at = |candidate_parent, state| {
                ExecInputBatch::from_signed_at_parent(&history, candidate_parent, parent, state, &rounds, &entries, 4, 16)
            };
            assert!(batch_at(parent, &close_parent_state).is_err());
            assert!(batch_at(original_state.last_point().unwrap().block, original_state).is_err());
            let candidate_batch = batch_at(parent, original_state).unwrap();
            assert_eq!(candidate_batch.root(), inputs.root());
            use kaspa_consensus_core::palw_state_v2::apply_delta_v2;
            let params = &node.bundle.state;
            let (published, mut delta) = node.vp().palw_state_v2_store.read().delta_of(anchor).unwrap();
            let original_delta = borsh::to_vec(&delta).unwrap();
            let reserve = |state, delta: &mut kaspa_consensus_core::palw_state_v2::PalwStateDeltaV2| {
                candidate_batch.reserve_closed_moe2_budget(&history, &node.config.params, &closure, &q, state, delta)
            };
            let wrong_parent = reserve(original_state, &mut delta).unwrap_err();
            assert!(wrong_parent.to_string().contains("close parent state"));
            assert_eq!(borsh::to_vec(&delta).unwrap(), original_delta);
            let mut wrong_position = delta.clone();
            wrong_position.point.blue_score += 1;
            let wrong_position = reserve(&close_parent_state, &mut wrong_position).unwrap_err();
            assert!(wrong_position.to_string().contains("differs from close position"));
            let error = reserve(&close_parent_state, &mut delta).unwrap_err();
            assert!(error.to_string().contains("local QF1 inference cannot reserve a mining reward"));
            assert_eq!(borsh::to_vec(&delta).unwrap(), original_delta);
            let post = apply_delta_v2(&close_parent_state, &delta, params).unwrap();
            assert_eq!(post.state_root(), published);
            for closed in [&closed_quote, &long] {
                let slot = closed.exec_slot();
                assert_eq!(slot.close, anchor);
                assert!(post.bond_budget().unwrap().claim_row(&slot.reservation_key()).is_none());
            }
        }
        let long_source = full[&10].consent;
        assert_ne!(long_source, full[&9].consent);
        assert_eq!(long_source, Hash64::from_bytes(entries[2].funding_ref));
        assert_eq!(full[&10].minimum_liability_sompi, full[&9].minimum_liability_sompi);
        let funding = node.chain.tip_state().1;
        let assignment_row = funding.bond_budget().unwrap().assignment_row(&quote.funding_bond).unwrap();
        assert_eq!(
            assignment_row.pending.as_deref(),
            Some(&[(quote.model, u64::try_from(2 * full[&9].minimum_liability_sompi).unwrap())][..])
        );
        assert!(node.chain.daa_of(assignment) < round.release_from_daa);
        let value = full[&9];
        assert_eq!(value.consent, Hash64::from_bytes(entries[1].funding_ref));
        assert_eq!(value.reward_sompi, quote.limits.max_reward);
        let policy = &node.bundle.state.bond_budget().unwrap().policy;
        let export = |held| kaspa_consensus_core::palw_bond_budget_v1::palw_bond_budget_export_cap_v1(policy, held);
        assert!(
            export(value.minimum_liability_sompi - 1) < value.reward_sompi
                && export(value.minimum_liability_sompi) >= value.reward_sompi
        );
        assert_eq!(value.collateral_sompi, close_parent_state.bond(&quote.funding_bond).unwrap().collateral);
        assert_ne!(value.consent, value.reservation_key);
        assert_eq!(value.reservation_key, closed_quote.exec_slot().reservation_key());
        assert_eq!(full[&10].reservation_key, long.exec_slot().reservation_key());
        assert!(close_parent_state.bond_budget().unwrap().claim_row(&value.reservation_key).is_none());
        assert!(quote.deadline < panel.release_from_daa);
        peer_log!(
            "MOE2 budget preview pid={} close={} parent_root={} slot=9 consent={} reservation={} collateral={} export_permille={} liability={} reward={} epoch={} panel_release={} bootstrap={} no_debit=true",
            std::process::id(),
            anchor,
            close_parent_state.state_root(),
            value.consent,
            value.reservation_key,
            value.collateral_sompi,
            policy.export_cap_permille,
            value.minimum_liability_sompi,
            value.reward_sompi,
            value.allocation_epoch,
            panel.release_from_daa,
            close_parent_state.bond_budget().unwrap().allocation_bootstrapping_v1()
        );
        let reward = quote.limits.max_reward;
        let split = closed_quote.preview(&node.config.params, original_state, reward).unwrap();
        let market = split.market.unwrap();
        assert_eq!((market.split.purchase, market.split.liquidity, market.split.workers), (reward * 3 / 10, reward / 5, reward / 2));
        assert_eq!((split.miner, split.panel_pool, market.retired_units), (reward * 4 / 10, reward / 10, 1));
        let (_, close_state) = node.chain.tip_state();
        assert!(closed_quote.preview(&node.config.params, &close_state, reward).is_err());
        assert!(closed_quote.preview(&node.config.params, original_state, reward + 1).is_err());
        assert_eq!(original_state.model_market(&quote.model), close_state.model_market(&quote.model));
        assert_eq!(
            original_state.bond_budget().unwrap().bond_row(&quote.funding_bond),
            close_state.bond_budget().unwrap().bond_row(&quote.funding_bond)
        );
        let before_market = original_state.model_market(&quote.model).unwrap();
        peer_log!(
            "MOE2 curve pid={} virtual={} reserve={} inventory={} purchased={} locked={}",
            std::process::id(),
            before_market.virtual_sompi,
            before_market.msk_reserve,
            before_market.position_units,
            market.purchased_units,
            market.liquidity_units
        );
        peer_log!(
            "MOE2 anchor preview pid={} parent_root={} buy={} lp={} burn={} miner={} panel={} original_bond_and_market_unchanged=true",
            std::process::id(),
            original_state.state_root(),
            market.split.purchase,
            market.split.liquidity,
            market.retired_units,
            split.miner,
            split.panel_pool
        );
        peer_log!(
            "MOE2 closed quote pid={} class={} preparation={} close={} quote_anchor={} deadline={} max_reward={} min_out={} consent={} no_debit=true",
            std::process::id(),
            quote.model,
            parent,
            anchor,
            quote.quote_anchor,
            quote.deadline,
            quote.limits.max_reward,
            quote.limits.min_out,
            Hash64::from_bytes(entries[1].funding_ref)
        );
        assert!(node.chain.tip_state().1.bond_budget().unwrap().claim_row(&Hash64::from_bytes(entries[1].funding_ref)).is_none());
        let err = inputs.observe(api, anchor, &closure, &q).err().expect("publication must wait for certified history");
        assert_eq!(err.kind(), std::io::ErrorKind::WouldBlock);
        let values = dealers.map(|d| (d, field(10 + d + slot + 1)));
        let row = Tb1HolderRelease {
            context: Tb1ReleaseContext { round_ref, qual_root: q.root(), freeze_id },
            holder_slot: slot,
            shares: &values,
        };
        Tb1ExposureJournal::provision(dir.path(), round_ref, &public[slot], slot as u32).unwrap();
        let mut journal = Tb1ExposureJournal::resume(dir.path(), round_ref, &public[slot], slot as u32).unwrap();
        let signing = journal.reserve_signing_message(api, freeze, &q, &row, 2048).unwrap_err();
        assert_eq!(signing.kind(), std::io::ErrorKind::WouldBlock);
        let observation = (model_block, parent, anchor, before.generation);
        peer_log!(
            "TB1 pending registered freeze pid={} slot={} registration={} preparation={} close={} generation={} daa={} from={} deadline={} publication={:?} signing={:?}",
            std::process::id(),
            slot,
            observation.0,
            parent,
            anchor,
            before.generation,
            daa,
            round.release_from_daa,
            round.deadline_exclusive_daa,
            err.kind(),
            signing.kind()
        );
        let mut parent_socket = child_dir.as_ref().map(|_| UnixStream::from(std::io::stdin().as_fd().try_clone_to_owned().unwrap()));
        let mut peers = Vec::new();
        if let Some(s) = &mut parent_socket {
            send(s, &observation);
        } else {
            peers.extend((1..3).map(|i| child_reader(dir.path(), i, true, true)));
            for (_, s) in &mut peers {
                assert_eq!(receive::<(Hash64, Hash64, Hash64, Hash64)>(s), observation);
            }
        }
        model_audit::registered_cut(
            &mut node,
            &mut parent_socket,
            &mut peers,
            dir.path(),
            child_dir.as_deref(),
            slot,
            roster,
            &keys,
            instance.as_ref().unwrap().identity(),
            round,
            &q,
        )
        .await;
        offers::observe(&node, original_state, &closed_quote, &long, assignment, panel.release_from_daa, long_source);
        if std::env::var_os("MISAKA_PRIVATE_MOE2_SEASON").is_some() {
            offers::season(&mut node, &mut parent_socket, &mut peers, original_state, &long, full[&10]).await;
        }
        for (child, _) in &mut peers {
            assert!(child.wait().unwrap().success());
        }
        return;
    }
    let (prefix, work, tip) = if let Some(dir) = &child_dir {
        let raw: [RpcRawBlock; 6] = serde_json::from_reader(File::open(dir.join("blocks.json")).unwrap()).unwrap();
        let [close, e1, e7, e2, work, tip] = raw.map(|r| Block::try_from(r).unwrap());
        let prefix = vec![close, e1, e7, e2];
        for block in &prefix {
            stable_panel::arrive(&mut node, block.clone(), "recorded close prefix").await;
        }
        let close = &prefix[0];
        plant_frontier(&node, close.header.blue_score, close.hash());
        stable_panel::arrive(&mut node, work.clone(), "recorded work").await;
        place_maturing_fact(&node, &work, 10, 0);
        node.vp().pruning_point_store.write().set(close.hash(), 1).unwrap();
        stable_panel::arrive(&mut node, tip.clone(), "recorded settlement tip").await;
        (prefix, work, tip)
    } else {
        // A close carrying an attempt opens a claim and cannot finalize before its lifecycle closes.
        let mut miner = new_miner_data();
        miner.extra_data = freeze_id.to_vec();
        let close = stable_panel::heartbeat(&mut node, miner);
        let close = node.take(close, "committed TB1 close").await;
        let mut prefix = vec![close];
        for card in [0, 7, 1] {
            prefix.push(stable_panel::next(&mut node, card).await);
        }
        let close = &prefix[0];
        plant_frontier(&node, close.header.blue_score, close.hash());
        let work = stable_panel::next(&mut node, 2).await;
        place_maturing_fact(&node, &work, 10, 0);
        node.vp().pruning_point_store.write().set(close.hash(), 1).unwrap();
        let tip = stable_panel::next(&mut node, 3).await;
        (prefix, work, tip)
    };
    let anchor = prefix[0].hash();
    let before = snapshot_of(&node).unwrap();
    assert_eq!(before.generation, tip.hash());
    assert_eq!(before.finalized, Some(anchor), "{}", serde_json::to_string(&readiness_of(&node)).unwrap());
    if stable {
        let start = tip.header.daa_score;
        if let Some(path) = &child_dir {
            let blocks: Vec<RpcRawBlock> =
                serde_json::from_reader(File::open(path.join("exec-release-window.json")).unwrap()).unwrap();
            for raw in blocks {
                stable_panel::arrive(&mut node, Block::try_from(raw).unwrap(), "wait for EXEC DAA").await;
            }
        } else {
            let mut blocks = vec![];
            while node.chain.daa_of(node.chain.sink()) < round.release_from_daa {
                assert!(blocks.len() < 120, "EXEC DAA did not progress from {start}");
                blocks.push(RpcRawBlock::from(&stable_panel::next(&mut node, 0).await));
            }
            serde_json::to_writer(File::create_new(dir.path().join("exec-release-window.json")).unwrap(), &blocks).unwrap();
        }
        peer_log!(
            "TB1 EXEC window pid={} first_daa={} release_daa={} observed_daa={}",
            std::process::id(),
            start,
            round.release_from_daa,
            node.chain.daa_of(node.chain.sink())
        );
    }
    let context = Tb1ReleaseContext { round_ref, qual_root: q.root(), freeze_id };
    let api = node.api().as_ref();
    let recover_intake = stable
        && qf1.is_some()
        && std::env::var_os("MISAKA_PRIVATE_QF1_NO_DISPUTE").is_some()
        && std::env::var_os("MISAKA_PRIVATE_QF1_CHECKPOINT_REPLAY").is_some()
        && std::env::var_os("MISAKA_PRIVATE_COLD_PANEL_WRITE").is_none();
    if recover_intake {
        node.api().checkpoint_database(&dir.path().join("before-exec-release"));
    }
    let jobs = inputs.observe(api, anchor, &closure, &q).unwrap();
    let gate = jobs.publication_gate();
    let freeze = gate.holder_freeze();
    let mut other_chain = round.chain_id;
    other_chain[0] ^= 1;
    assert!(!api.palw_coinbase_commits_to(other_chain, anchor, &freeze_id).unwrap());
    for changed in [
        Tb1MiningClosure { consumer_set_root: [65; 64], ..closure },
        Tb1MiningClosure { policy_root: [65; 64], ..closure },
        Tb1MiningClosure { round: Tb1MiningRound { purpose: Tb1MiningPurpose::Panel, ..round }, ..closure },
        Tb1MiningClosure { round: Tb1MiningRound { deadline_exclusive_daa: round.deadline_exclusive_daa + 1, ..round }, ..closure },
    ] {
        assert!(Tb1HolderFreeze::resolve(api, anchor, &changed, &q).is_err());
    }
    assert!(Tb1HolderFreeze::resolve(api, tip.hash(), &closure, &q).is_err());
    let shares = |slot: usize| dealers.map(|d| (d, field(10 + d + slot + 1)));
    let values = shares(slot);
    let row = Tb1HolderRelease { context, holder_slot: slot, shares: &values };
    Tb1ExposureJournal::provision(dir.path(), round_ref, &public[slot], slot as u32).unwrap();
    let mut journal = Tb1ExposureJournal::resume(dir.path(), round_ref, &public[slot], slot as u32).unwrap();
    let msg = journal.reserve_signing_message(api, freeze, &q, &row, 2048).unwrap();
    let sig = ml_dsa_65::sign(&keys[slot].signing_key, &msg, RELEASE_CONTEXT, [71; 32]).unwrap();
    peer_log!(
        "TB1 native acceptance pid {}, slot {slot}, anchor {anchor}, sink {}, qualification {}",
        std::process::id(),
        before.generation,
        Hash64::from_bytes(q.root())
    );
    let own_record = ReleaseRecord {
        holder: slot,
        shares: values.iter().map(|(d, v)| (*d, v.to_bytes().to_vec())).collect(),
        signature: sig.as_slice().to_vec(),
    };
    let mut peers = Vec::new();
    let mut parent_socket = None;
    let (records, batch, certificate) = if child_dir.is_some() {
        let mut socket = UnixStream::from(std::io::stdin().as_fd().try_clone_to_owned().unwrap());
        send(&mut socket, &own_record);
        let publication: Publication = receive(&mut socket);
        let batch = Block::try_from(publication.batch).unwrap();
        let certificate = Block::try_from(publication.certificate).unwrap();
        stable_panel::arrive(&mut node, batch.clone(), "native signed batch").await;
        parent_socket = Some(socket);
        (publication.rows, batch, certificate)
    } else {
        let raw: Vec<_> = prefix.iter().chain([&work, &tip]).map(RpcRawBlock::from).collect();
        serde_json::to_writer(File::create_new(dir.path().join("blocks.json")).unwrap(), &raw).unwrap();
        peers.extend((1..3).map(|s| child_reader(dir.path(), s, stable, false)));
        let mut records = vec![own_record];
        records.extend(peers.iter_mut().map(|(_, s)| receive::<ReleaseRecord>(s)));
        let decoded: Vec<_> = records.iter().map(ReleaseRecord::decode).collect();
        let rows = views(&decoded, context);
        let miner = MinerData::new(card_payout_spk(4), batch_commitment(&rows, LIMITS).unwrap().to_vec());
        let template = stable_panel::carrier(&mut node, 4, miner);
        let batch = node.take(template, "TB1 signed batch").await;
        let marker = gate.propose(node.api().as_ref(), batch.hash(), &rows, &q, LIMITS).unwrap();
        assert!(gate.read_seed(node.api().as_ref(), batch.hash(), batch.hash(), &rows, &q, LIMITS).is_err());
        let miner = MinerData::new(card_payout_spk(5), marker.to_vec());
        let template = stable_panel::carrier(&mut node, 5, miner);
        let certificate = template.to_immutable();
        let publication = Publication { batch: (&batch).into(), certificate: (&certificate).into(), rows: records };
        for (_, socket) in &mut peers {
            send(socket, &publication);
        }
        (publication.rows, batch, certificate)
    };
    let decoded: Vec<_> = records.iter().map(ReleaseRecord::decode).collect();
    let mut rows = views(&decoded, context);
    assert_eq!(node.chain.sink(), batch.hash());
    stable_panel::arrive(&mut node, certificate.clone(), "native TB1 certificate").await;
    assert_eq!(node.chain.sink(), certificate.hash());
    let api = node.api().as_ref();
    let seed = gate.read_seed(api, batch.hash(), certificate.hash(), &rows, &q, LIMITS).unwrap();
    assert!(gate.read_seed(api, batch.hash(), certificate.hash(), &rows[..2], &q, LIMITS).is_err());
    rows.swap(0, 1);
    assert!(gate.read_seed(api, batch.hash(), certificate.hash(), &rows, &q, LIMITS).is_err());
    rows.swap(0, 1);
    peer_log!(
        "TB1 publication pid {} batch {} certificate {} seed {}",
        std::process::id(),
        batch.hash(),
        certificate.hash(),
        faster_hex::hex_string(seed.as_bytes())
    );
    let request = jobs.derive(api, 7, batch.hash(), certificate.hash(), &rows, &q, LIMITS).unwrap();
    assert_eq!(request.registration_key(), &key);
    assert_eq!(request.tokens().len(), 38);
    assert_eq!(&request.tokens()[35..], terms.job_tokens);
    assert!(jobs.derive(api, 8, batch.hash(), certificate.hash(), &rows, &q, LIMITS).is_err());
    let mut retained_qf1 = None;
    if let Some(terms) = &qf1_terms {
        let r = jobs.derive(api, 9, batch.hash(), certificate.hash(), &rows, &q, LIMITS).unwrap();
        assert_eq!(r.registration_key(), &terms.registration_key(parent, &rounds, &owner.verification_key, 16).unwrap());
        assert_ne!(r.attempt(), request.attempt());
        // One write keeps the record intact when all three processes report at once.
        let line =
            format!("QF1 input pid={} attempt={} tokens={:?}\n", std::process::id(), faster_hex::hex_string(r.attempt()), r.tokens());
        std::io::stderr().lock().write_all(line.as_bytes()).unwrap();
        if let Some(path) = std::env::var_os("MISAKA_PRIVATE_QF1_EMBEDDING") {
            embedding::check_input(Path::new(&path), r.tokens());
        }
        let bound = instance.as_ref().unwrap();
        let (artifact, norm) = exercise.as_ref().unwrap();
        let substituted = prediction::bind_scale(artifact, *norm, b"other scale");
        assert!(matches!(r.replay_qf1_local(api, &substituted), Err(e) if e.contains("owner-signed registration")));
        let work = r.qf1_computational_work_key(api, bound).unwrap();
        assert!(r.qf1_computational_work_key(api, &substituted).is_err());
        assert!(request.qf1_computational_work_key(api, bound).is_err());
        if full.is_none() {
            assert_ne!(bound.identity().descriptor, substituted.identity().descriptor);
            let limit = r.terms().decode_tokens;
            assert_eq!(substituted.computational_work_key(r.tokens(), limit).unwrap(), work);
            assert_ne!(bound.computational_work_key(r.tokens(), limit + 1).unwrap(), work);
            let mut different = r.tokens().to_vec();
            different[0] = 0;
            assert_ne!(bound.computational_work_key(&different, limit).unwrap(), work);
        }
        peer_log!("QF1 numerical work key pid={} key={work}", std::process::id());
        assert!(request.replay_qf1_local(api, bound).is_err());
        let (replay, opened) = if let Some(s) = parent_socket.as_mut() {
            let declared = receive::<u32>(s);
            let opened = if slot == 1 {
                64
            } else {
                bound.trace_leaf_count(r.tokens().len().try_into().unwrap(), declared, terms.max_leaves).unwrap() - 1
            };
            (r.replay_qf1_original(api, bound, declared, Some(opened)).unwrap(), opened)
        } else {
            let replay = r.produce_qf1_original(api, bound, Some(0)).unwrap();
            let t = replay.trace().answer.tokens.len() as u32;
            for (_, s) in &mut peers {
                send(s, &t);
            }
            peer_log!("QF1 one-pass producer pid={} attempt={} length={}", std::process::id(), faster_hex::hex_string(r.attempt()), t);
            (replay, 0)
        };
        let answer = &replay.trace().answer;
        if full.is_none() {
            prediction::check(r.tokens(), terms.decode_tokens, answer);
            assert_eq!(answer.tokens, [7, 151643]);
        }
        let (body, sig, stop_material, end_material) = if let Some(s) = parent_socket.as_mut() {
            let (body, bytes, stop, end) =
                receive::<(misaka_palw_remote::tb1_exposure::exec_input::LocalOriginalBody, Vec<u8>, Vec<u8>, Vec<u8>)>(s);
            (body, ml_dsa_65::MLDSA65Signature::new(bytes.as_slice().try_into().unwrap()), stop, end)
        } else {
            let mut body = replay.body();
            if inconsistent_readout_fixture() {
                use kaspa_consensus_core::palw_qf1::{TreeKind, TreeRoot, ticket};
                // Negative control: the signed readout commits zero while the
                // published terminal records still contain the real execution.
                assert_ne!(replay.trace().answer.z, [0; 64]);
                let mut roots = replay.trace().roots;
                roots[2] = TreeRoot::build(*r.attempt(), TreeKind::Readout, std::iter::once([0u8; 64].as_slice()), 1).unwrap();
                body.roots[2] = Hash64::from_bytes(roots[2].digest);
                body.ticket = ticket(roots.each_ref()).unwrap();
            }
            let id = body.signing_id(&r);
            let sig = ml_dsa_65::sign(
                &owner.signing_key,
                id.as_byte_slice(),
                misaka_palw_remote::tb1_exposure::exec_input::LOCAL_ORIGINAL_DOMAIN,
                [94; 32],
            )
            .unwrap();
            let stop = replay.stop_witness().unwrap().encode();
            let end = replay.end_disclosure().unwrap();
            for (_, s) in &mut peers {
                send(s, &(&body, sig.as_slice().to_vec(), &stop, &end));
            }
            (body, sig, stop, end)
        };
        assert!(r.verify_qf1_stop_disclosure(api, &body, &sig, &[]).is_err());
        assert!(r.verify_qf1_end_disclosure(api, &body, &sig, &[]).is_err());
        let (end_original, end_disposition) = r.verify_qf1_end_disclosure(api, &body, &sig, &end_material).unwrap();
        let (original, disposition) = r.verify_qf1_stop_disclosure(api, &body, &sig, &stop_material).unwrap();
        // Preserve what the owner committed before comparing it to this peer's inference.
        assert_eq!(original, end_original);
        for (suffix, material) in [("stop", &stop_material), ("end", &end_material)] {
            let mut file = File::create_new(dir.path().join(format!("{original}.{suffix}"))).unwrap();
            file.write_all(material).unwrap();
            file.sync_all().unwrap();
        }
        File::open(dir.path()).unwrap().sync_all().unwrap();
        peer_log!("QF1 retained Stop pid={} original={} bytes={}", std::process::id(), original, stop_material.len());
        peer_log!("QF1 retained End pid={} original={} bytes={}", std::process::id(), original, end_material.len());
        use misaka_palw_remote::tb1_exposure::exec_input::stop_case::StopOutcome;
        assert_eq!(disposition, StopOutcome::ChallengeNotEstablished);
        if inconsistent_readout_fixture() {
            assert_eq!(end_disposition, StopOutcome::ComputationRefuted);
            assert!(replay.verify(api, &body, &sig).unwrap_err().to_string().contains("different replayed"));
        } else {
            assert_eq!(end_disposition, StopOutcome::ChallengeNotEstablished);
            replay.verify(api, &body, &sig).unwrap();
        }
        let mut changed = body.clone();
        changed.roots[3] = Hash64::from_u64_word(1);
        assert!(replay.verify(api, &changed, &sig).is_err());
        assert!(r.verify_qf1_body(api, &changed, &sig).is_err());
        assert!(replay.verify(api, &body, &owner_sig).is_err());
        assert!(r.read_qf1_original(api, certificate.hash(), &body, &sig).is_err());
        let trace = replay.trace();
        let opening = trace.opening.as_ref().unwrap();
        assert!(trace.roots[3].verify_opening(opened, &opening.record, &opening.siblings));
        assert!(!trace.roots[3].verify_opening(opened, b"wrong", &opening.siblings));
        let mut signed_ticket = [0u8; 33];
        signed_ticket[1..].copy_from_slice(&body.ticket);
        let observed = serde_json::json!({ "pid":std::process::id(),"original":original,"roots":body.roots,
            "counts":body.counts,"ticket":faster_hex::hex_string(&body.ticket),
            "target":faster_hex::hex_string(&terms.target),"ticket_meets_target":signed_ticket < terms.target});
        let witness = serde_json::json!({"pid":std::process::id(),"index":opened,"record":faster_hex::hex_string(&opening.record),
            "siblings":opening.siblings.iter().map(|h|Hash64::from_bytes(*h)).collect::<Vec<_>>()});
        for line in [format!("QF1 original {observed}\n"), format!("QF1 disclosure {witness}\n")] {
            std::io::stderr().lock().write_all(line.as_bytes()).unwrap();
        }
        let report = (answer.tokens.clone(), answer.processed_positions, Hash64::from_bytes(answer.z));
        let line = format!(
            "QF1 fixture pid={} class={} descriptor={} positions={} tokens={:?} stop={:?} z={}\n",
            std::process::id(),
            bound.identity().class,
            bound.identity().descriptor,
            report.1,
            report.0,
            answer.stop,
            report.2
        );
        std::io::stderr().lock().write_all(line.as_bytes()).unwrap();
        if let Some(socket) = parent_socket.as_mut() {
            send(socket, &report);
            send(socket, &r.tokens());
        }
        for (_, socket) in &mut peers {
            assert_eq!(receive::<(Vec<u32>, u32, Hash64)>(socket), report);
            assert_eq!(receive::<Vec<u32>>(socket), r.tokens());
        }
        retained_qf1 = Some((r, body, sig, original));
    }
    let ids = request.tokens().to_vec();
    peer_log!(
        "TB1 EXEC input pid {} slot 7 attempt {} tokens {:?}",
        std::process::id(),
        faster_hex::hex_string(request.attempt()),
        ids
    );
    if let Some(socket) = parent_socket.as_mut() {
        socket.write_all(seed.as_bytes()).unwrap();
    }
    if stable {
        for (_, socket) in &mut peers {
            let mut bytes = [0; 32];
            socket.read_exact(&mut bytes).unwrap();
            assert_eq!(&bytes, seed.as_bytes());
        }
    }
    let execution = std::env::var_os("MISAKA_PRIVATE_TB1_TIR_ARTIFACT").map(|path| exec_input::infer(api, Path::new(&path), &request));
    if let Some(run) = &execution {
        peer_log!("TB1 TIR inference pid {} {}", std::process::id(), serde_json::to_string(run).unwrap());
    }
    let original_receipt = if let Some((r, body, sig, id)) = &retained_qf1 {
        let block = local_carrier(&mut node, &mut parent_socket, &mut peers, 6, id.as_bytes(), "local Original receipt").await;
        assert_eq!(r.read_qf1_original(node.api().as_ref(), block.hash(), body, sig).unwrap(), *id);
        peer_log!("QF1 receipt pid={} block={} original={}", std::process::id(), block.hash(), id);
        use misaka_palw_remote::tb1_exposure::exec_input::panel_cut::{LocalPanelInputs, LocalSubmission, RecordedLocalSubmission};
        let actors = stable.then(|| stable_panel::Actors::enroll(&mut parent_socket, &mut peers));
        let candidates = actors.as_ref().map_or_else(
            || vec![(Hash64::from_u64_word(1), 1), (Hash64::from_u64_word(2), 2), (Hash64::from_u64_word(3), 4)],
            |a| a.candidates(),
        );
        let draw_policy = actors.as_ref().map_or(Hash64::from_u64_word(72), |a| a.policy.id());
        let reservation = Hash64::from_u64_word(71);
        let submission = || LocalSubmission { input: r, block: block.hash(), body, signature: sig, material_reservation: reservation };
        let pool = |policy| LocalPanelInputs { draw_policy: policy, candidates: &candidates, max_candidates: 3 };
        let api = node.api().as_ref();
        let cut = jobs
            .local_panel_cut(api, batch.hash(), certificate.hash(), &rows, &q, LIMITS, &[submission()], pool(draw_policy))
            .unwrap();
        let other = jobs
            .local_panel_cut(
                api,
                batch.hash(),
                certificate.hash(),
                &rows,
                &q,
                LIMITS,
                &[submission()],
                pool(Hash64::from_u64_word(73)),
            )
            .unwrap();
        let empty = jobs.local_panel_cut(api, batch.hash(), certificate.hash(), &rows, &q, LIMITS, &[], pool(draw_policy)).unwrap();
        assert!(
            jobs.local_panel_cut(
                api,
                batch.hash(),
                certificate.hash(),
                &rows,
                &q,
                LIMITS,
                &[submission(), submission()],
                pool(draw_policy)
            )
            .is_err()
        );
        if stable {
            while node.api().get_header(node.chain.sink()).unwrap().blue_score + 1 < panel.close_blue_score {
                local_carrier(&mut node, &mut parent_socket, &mut peers, 0, [80; 64], "PANEL cutoff heartbeat").await;
            }
        }
        let close_parent = node.chain.sink();
        let marker = cut.commitment(close_parent);
        assert_ne!(marker, other.commitment(close_parent));
        assert_ne!(marker, empty.commitment(close_parent));
        let closed = local_carrier(&mut node, &mut parent_socket, &mut peers, 7, marker, "local PANEL close").await;
        let api = node.api().as_ref();
        cut.resolve(api, closed.hash(), &panel_q).unwrap();
        assert!(cut.resolve(api, closed.hash(), &q).is_err());
        assert!(other.resolve(api, closed.hash(), &panel_q).is_err());
        assert!(empty.resolve(api, closed.hash(), &panel_q).is_err());
        assert!(cut.observe(api, closed.hash(), &panel_q).is_err());
        peer_log!(
            "QF1 panel close pid={} receipt={} close={} blue={} marker={} release=pending",
            std::process::id(),
            block.hash(),
            closed.hash(),
            closed.header.blue_score,
            Hash64::from_bytes(marker)
        );
        if let (Some(setup), Some(actors)) = (setup.as_ref(), actors.as_ref()) {
            stable_panel::release(
                &mut node,
                &mut parent_socket,
                &mut peers,
                dir.path(),
                slot,
                &keys[slot],
                &panel_q,
                &panel_id,
                &panel_ready,
                &cut,
                close_parent,
                block.hash(),
                &closed,
                panel.release_from_daa,
                setup,
                seed.as_bytes(),
                &rows,
                actors,
                r,
                |api| jobs.derive_recorded(api, 9, batch.hash(), certificate.hash(), &rows, &q, LIMITS).unwrap(),
                instance.as_ref().unwrap(),
                body,
                sig,
                *id,
            )
            .await;
            if recover_intake {
                let observed = stable_panel::reopen_checkpoint(&node.config, &dir.path().join("before-exec-release"));
                let observed_api = observed.as_ref();
                let retained = ExecInputBatch::from_signed(observed_api, parent, &rounds, &entries, 4, 16).unwrap();
                let recovered_exec = retained.observe(observed_api, anchor, &closure, &q).unwrap();
                drop(observed);
                let expired = stable_panel::reopen_checkpoint(&node.config, &dir.path().join("at-original-deadline"));
                let api = expired.as_ref();
                let recorded =
                    RecordedLocalSubmission { slot: 9, block: block.hash(), body, signature: sig, material_reservation: reservation };
                assert!(r.check_live(api).is_err());
                assert!(r.read_qf1_original(api, block.hash(), body, sig).is_err());
                assert!(
                    jobs.local_panel_cut(api, batch.hash(), certificate.hash(), &rows, &q, LIMITS, &[submission()], pool(draw_policy))
                        .is_err()
                );
                let rebuild = |rows_at_cut: &[RecordedLocalSubmission<'_>]| {
                    recovered_exec.recorded_panel_cut(
                        api,
                        batch.hash(),
                        certificate.hash(),
                        &rows,
                        &q,
                        LIMITS,
                        rows_at_cut,
                        pool(draw_policy),
                    )
                };
                let restored = rebuild(&[recorded]).unwrap();
                assert_eq!(restored.consumer_set_root(), cut.consumer_set_root());
                assert!(api.palw_coinbase_commits_to(round.chain_id, closed.hash(), &restored.commitment(close_parent)).unwrap());
                assert!(rebuild(&[RecordedLocalSubmission { slot: 7, ..recorded }]).is_err());
                assert!(rebuild(&[RecordedLocalSubmission { block: api.get_sink(), ..recorded }]).is_err());
                let mut changed = body.clone();
                changed.ticket[0] ^= 1;
                assert!(rebuild(&[RecordedLocalSubmission { body: &changed, ..recorded }]).is_err());
                peer_log!(
                    "QF1 reconstructed PANEL cut after deadline pid={} original={} close={}",
                    std::process::id(),
                    id,
                    closed.hash()
                );
            }
            if let Some(root) = std::env::var_os("MISAKA_PRIVATE_COLD_PANEL_WRITE") {
                let entry = entries
                    .iter()
                    .find(|e| e.terms.registration_key(parent, &rounds, e.owner, 16).unwrap() == *r.registration_key())
                    .unwrap();
                let intake = stable_panel::ColdPanelIntake {
                    exec_inputs: Hash64::from_bytes(closure.consumer_set_root),
                    exec_close: anchor,
                    exec_certificate: certificate.hash(),
                    exec_seed: *seed.as_bytes(),
                    candidates,
                    rows: vec![stable_panel::IntakeRow {
                        slot: entry.slot,
                        registration: Hash64::from_bytes(*r.registration_key()),
                        original: *id,
                        block: block.hash(),
                        funding: Hash64::from_bytes(entry.funding_ref),
                        material: reservation,
                        deadline: entry.terms.deadline_exclusive_daa,
                    }],
                };
                let path = std::path::PathBuf::from(root).join(slot.to_string()).join("intake.json");
                std::fs::write(path, serde_json::to_vec(&intake).unwrap()).unwrap();
            }
        }
        Some((block, closed, cut))
    } else {
        None
    };
    if let Some(path) = child_dir {
        serde_json::to_writer(File::create_new(path.join(format!("result-{slot}.json"))).unwrap(), &(ids, execution)).unwrap();
        return;
    }
    for (idx, (child, socket)) in peers.iter_mut().enumerate() {
        if !stable {
            let mut received = [0; 32];
            socket.read_exact(&mut received).unwrap();
            assert_eq!(&received, seed.as_bytes());
        }
        assert!(child.wait().unwrap().success());
        let peer: (Vec<u32>, Option<exec_input::Inference>) =
            serde_json::from_reader(File::open(dir.path().join(format!("result-{}.json", idx + 1))).unwrap()).unwrap();
        assert_eq!(peer.0, ids);
        assert_eq!(peer.1, execution);
    }
    if stable {
        assert!(original_receipt.is_some());
        return;
    }
    let later = match original_receipt {
        Some((ref block, _, _)) => block.clone(),
        None => extend(&mut node, &[6]).await.remove(0),
    };
    let api = node.api().as_ref();
    assert_eq!(gate.read_seed(api, batch.hash(), certificate.hash(), &rows, &q, LIMITS).unwrap(), seed);
    assert!(gate.read_seed(api, batch.hash(), later.hash(), &rows, &q, LIMITS).is_err());
    let mut history = vec![preparation];
    history.extend_from_slice(&prefix);
    let mut rival = follower(&parts, 0x12_d500_0000, &history).await;
    plant_frontier(&rival, prefix[0].header.blue_score, anchor);
    let fork_cards: &[usize] = if original_receipt.is_some() { &[4, 5, 6, 0, 1, 2, 3] } else { &[4, 5, 6, 0, 1, 2] };
    let alternative = extend(&mut rival, fork_cards).await;
    for block in &alternative {
        stable_panel::arrive(&mut node, block.clone(), "heavier branch").await;
    }
    let after = snapshot_of(&node).unwrap();
    assert_eq!(after.generation, alternative.last().unwrap().hash());
    assert!(node.api().is_chain_ancestor_of(anchor, after.generation).unwrap());
    assert_eq!(after.finalized, None);
    assert!(request.check_live(node.api().as_ref()).is_err());
    if let Some((_, closed, cut)) = &original_receipt {
        assert!(cut.resolve(node.api().as_ref(), closed.hash(), &panel_q).is_err());
    }
    if let Some((r, body, sig, _)) = &retained_qf1 {
        assert!(r.read_qf1_original(node.api().as_ref(), later.hash(), body, sig).is_err());
        assert!(
            matches!(r.replay_qf1_local(node.api().as_ref(), instance.as_ref().unwrap()), Err(e) if e.contains("live certificate"))
        );
    }
    assert!(gate.read_seed(node.api().as_ref(), batch.hash(), certificate.hash(), &rows, &q, LIMITS).is_err());
    assert_eq!(readiness_of(&node).finalized.withdrawn_from, Some(anchor));
    assert!(journal.observe_branch(node.api().as_ref()).unwrap_err().to_string().contains("halted"));
    drop(journal);
    assert!(
        matches!(Tb1ExposureJournal::resume(dir.path(),context.round_ref,&public[0],0),Err(err) if err.to_string().contains("halted"))
    );
    peer_log!(
        "TB1 native custody: anchor {anchor}, prior sink {}, replacement {}, stop {:?}",
        before.generation,
        after.generation,
        after.stop
    );
}
