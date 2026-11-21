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

async fn local_carrier(
    node: &mut Rig,
    parent: &mut Option<UnixStream>,
    peers: &mut [(Child, UnixStream)],
    card: usize,
    marker: [u8; 64],
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
    qualification_from_ready(round_ref, roster, dealers, session, transcripts, |_, r| {
        ml_dsa_65::sign(&keys[r.holder_slot].signing_key, &ready_message(r, 2048).unwrap(), READY_CONTEXT, [9; 32])
            .unwrap()
            .as_slice()
            .to_vec()
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

#[tokio::test]
#[ignore = "requires the full registered QF1 artifact and tokenizer"]
async fn private_registered_tb1_pending_freeze() {
    assert!(std::env::var_os("MISAKA_PRIVATE_QF1_FULL").is_some());
    run(true, true).await;
}

async fn run(stable: bool, registered: bool) {
    use super::super::g14_registration_e2e::{private_moe2::arm_private_registration, tb1_registration_prefix};
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
    let child_dir = std::env::var_os(READER_DIR).map(std::path::PathBuf::from);
    let dir = kaspa_database::utils::get_kaspa_tempdir();
    let full = std::env::var_os("MISAKA_PRIVATE_QF1_FULL");
    let tokenizer = std::env::var_os("MISAKA_PRIVATE_QF1_TOKENIZER");
    assert!(full.is_none() || (stable && tokenizer.is_some()), "the full class uses the stable round and pinned tokenizer");
    let held = full.as_ref().map(|p| misaka_palw_tir_exec::node::TirArtifactV1::open(Path::new(p)).unwrap());
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
    let registration = if registered {
        let artifact = held.as_ref().unwrap();
        let class = artifact.class().unwrap();
        let inventory = artifact.inventory_root().unwrap().0;
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
    let panel_q = qualified(&panel_id, roster, &keys, &dealers, 29, setup.as_ref().map(|p| &p.roots));
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
    let entry =
        SignedExecInput { terms: &terms, owner: &owner.verification_key, signature: &owner_sig, slot: 7, funding_ref: [23; 64] };
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
    // Optional non-rewarding RFC-0066 exercise, with the real pinned tokenizer.
    let exercise = tokenizer.as_ref().map(|_| match held {
        Some(a) => (a, misaka_palw_sdk::qf1_local::program().unwrap().final_norm),
        None => prediction::artifact(&dir.path().join("schedule.palwtir")),
    });
    let instance = exercise.as_ref().map(|(a, norm)| match &full {
        Some(_) => misaka_palw_sdk::qf1_local::bind(a).unwrap(),
        None => prediction::bind(a, *norm),
    });
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
        Qf1Prompt::from_registered_tokenizer(
            &bytes,
            expected,
            instance.as_ref().unwrap().identity().descriptor,
            "2 + 2?",
            16_000_000,
            128,
        )
        .unwrap()
    });
    let qf1_terms = qf1.as_ref().map(|prompt| misaka_palw_remote::tb1_exposure::exec_input::ExecInputTerms {
        qf1: Some(Qf1Use::LocalInference(prompt)),
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
        decode_tokens: 3,
        max_leaves: if full.is_some() { 32_000_000 } else { terms.max_leaves },
        ..terms
    });
    if let (Some(prompt), Some(terms)) = (&qf1, &qf1_terms) {
        let mining = misaka_palw_remote::tb1_exposure::exec_input::ExecInputTerms { qf1: Some(Qf1Use::NewMining(prompt)), ..*terms };
        assert!(
            mining.registration_key(parent, &rounds, &owner.verification_key, 16).unwrap_err().to_string().contains("REVIEW_HOLD")
        );
    }
    let mut entries = vec![entry];
    let qf1_sig;
    if let Some(terms) = &qf1_terms {
        let key = terms.registration_key(parent, &rounds, &owner.verification_key, 16).unwrap();
        // Readers verify the producer's original signature.
        let file = child_dir.as_deref().unwrap_or(dir.path()).join("qf1.sig");
        qf1_sig = if child_dir.is_some() {
            ml_dsa_65::MLDSA65Signature::new(std::fs::read(file).unwrap().as_slice().try_into().unwrap())
        } else {
            let sig = ml_dsa_65::sign(&owner.signing_key, &key, REGISTRATION_DOMAIN, [85; 32]).unwrap();
            std::fs::write(file, sig.as_slice()).unwrap();
            sig
        };
        entries.push(SignedExecInput { terms, owner: &owner.verification_key, signature: &qf1_sig, slot: 9, funding_ref: [24; 64] });
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
        let file = "pending-freeze-blocks.json";
        let anchor = if let Some(dir) = &child_dir {
            let raw: Vec<RpcRawBlock> = serde_json::from_reader(File::open(dir.join(file)).unwrap()).unwrap();
            let mut blocks = raw.into_iter().map(|r| Block::try_from(r).unwrap());
            let close = blocks.next().unwrap();
            let anchor = close.hash();
            stable_panel::arrive(&mut node, close, "registered EXEC close").await;
            for block in blocks {
                stable_panel::arrive(&mut node, block, "registered release window").await;
            }
            anchor
        } else {
            let mut miner = new_miner_data();
            miner.extra_data = freeze_id.to_vec();
            let t = stable_panel::heartbeat(&mut node, miner);
            let close = node.take(t, "registered EXEC close").await;
            let mut blocks = vec![RpcRawBlock::from(&close)];
            while node.chain.daa_of(node.chain.sink()) < round.release_from_daa {
                assert!(blocks.len() < 120, "native release DAA did not advance");
                blocks.push(RpcRawBlock::from(&stable_panel::next(&mut node, 0).await));
            }
            serde_json::to_writer(File::create_new(dir.path().join(file)).unwrap(), &blocks).unwrap();
            close.hash()
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
        assert!(request.replay_qf1_local(api, bound).is_err());
        let answer = r.replay_qf1_local(api, bound).unwrap();
        if full.is_none() {
            prediction::check(r.tokens(), terms.decode_tokens, &answer);
            assert_eq!(answer.tokens, [7, 151643]);
        }
        // Only the producer signs the Original. Readers use its declared T
        // for the tree length and independently execute and authenticate it.
        let declared = if let Some(s) = parent_socket.as_mut() {
            receive::<u32>(s)
        } else {
            let t = answer.tokens.len() as u32;
            for (_, s) in &mut peers {
                send(s, &t);
            }
            t
        };
        let opened = match slot {
            0 => 0,
            1 => 64,
            _ => bound.trace_leaf_count(r.tokens().len().try_into().unwrap(), declared, terms.max_leaves).unwrap() - 1,
        };
        let replay = r.replay_qf1_original(api, bound, declared, Some(opened)).unwrap();
        let (body, sig) = if let Some(s) = parent_socket.as_mut() {
            let (body, bytes) = receive::<(misaka_palw_remote::tb1_exposure::exec_input::LocalOriginalBody, Vec<u8>)>(s);
            (body, ml_dsa_65::MLDSA65Signature::new(bytes.as_slice().try_into().unwrap()))
        } else {
            let id = replay.id();
            let sig = ml_dsa_65::sign(
                &owner.signing_key,
                id.as_byte_slice(),
                misaka_palw_remote::tb1_exposure::exec_input::LOCAL_ORIGINAL_DOMAIN,
                [94; 32],
            )
            .unwrap();
            for (_, s) in &mut peers {
                send(s, &(replay.body(), sig.as_slice().to_vec()));
            }
            (replay.body(), sig)
        };
        let original = r.verify_qf1_body(api, &body, &sig).unwrap();
        replay.verify(api, &body, &sig).unwrap();
        let mut changed = body.clone();
        changed.roots[3] = Hash64::from_u64_word(1);
        assert!(replay.verify(api, &changed, &sig).is_err());
        assert!(r.verify_qf1_body(api, &changed, &sig).is_err());
        assert!(replay.verify(api, &body, &owner_sig).is_err());
        assert!(r.read_qf1_original(api, certificate.hash(), &body, &sig).is_err());
        let trace = replay.trace();
        assert_eq!(trace.answer.tokens, answer.tokens);
        assert_eq!(trace.answer.z, answer.z);
        let opening = trace.opening.as_ref().unwrap();
        assert!(trace.roots[3].verify_opening(opened, &opening.record, &opening.siblings));
        assert!(!trace.roots[3].verify_opening(opened, b"wrong", &opening.siblings));
        let observed = serde_json::json!({ "pid":std::process::id(),"original":original,"roots":trace.roots.map(|r|Hash64::from_bytes(r.digest)),
            "counts":trace.roots.map(|r|r.count()),"ticket":faster_hex::hex_string(&trace.ticket),
            "target":faster_hex::hex_string(&terms.target),"ticket_meets_target":replay.ticket_meets_target().unwrap()});
        let witness = serde_json::json!({"pid":std::process::id(),"index":opened,"record":faster_hex::hex_string(&opening.record),
            "siblings":opening.siblings.iter().map(|h|Hash64::from_bytes(*h)).collect::<Vec<_>>()});
        for line in [format!("QF1 original {observed}\n"), format!("QF1 disclosure {witness}\n")] {
            std::io::stderr().lock().write_all(line.as_bytes()).unwrap();
        }
        let report = (answer.tokens, answer.processed_positions, Hash64::from_bytes(answer.z));
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
        use misaka_palw_remote::tb1_exposure::exec_input::panel_cut::{LocalPanelInputs, LocalSubmission};
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
                instance.as_ref().unwrap(),
                body,
                sig,
                *id,
            )
            .await;
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
