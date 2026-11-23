//! Isolated heartbeat-only continuation of the EXEC/Original fixture. Ordinary
//! native reads certify the close. The settlement work and dealer provisioning
//! are test inputs, independent of the new claim's inference or rewards.
use super::*;
use kaspa_consensus_core::palw_tb1::{Tb1DealerPoint, reconstruction::reconstruct_dealer};
use kaspa_math::Uint576;
use misaka_palw_remote::tb1_exposure::exec_input::panel_cut::LocalPanelCut;
use misaka_palw_remote::tb1_exposure::exec_input::{
    BranchExecInput, DerivedExecInput, LocalOriginalBody, Tb1HistoryAction, Tb1HistoryError,
    panel_cut::{LocalReplayPolicy, REPLAY_SIGNATURE_DOMAIN, actor_id},
};
use misaka_palw_tir_exec::node::qf1::LocalInstance;
use rand::{RngCore, rngs::OsRng};
use sha2::{Digest, Sha512};

pub(super) fn arm(node: &mut Rig) {
    let clock = node.chain.ctx.simulated_time;
    node.chain.ctx.consensus.set_validation_time_for_test(clock);
    node.vp().template_clock.store(clock, std::sync::atomic::Ordering::Relaxed);
}

fn clocked(node: &Rig) -> bool {
    node.chain.ctx.consensus.has_validation_time_for_test()
}

pub(super) fn heartbeat(node: &mut Rig, miner: MinerData) -> MutableBlock {
    heartbeat_with_transactions(node, miner, vec![])
}

pub(super) fn heartbeat_with_transactions(
    node: &mut Rig,
    miner: MinerData,
    txs: Vec<kaspa_consensus_core::tx::Transaction>,
) -> MutableBlock {
    if clocked(node) {
        node.chain.ctx.simulated_time += node.config.params.target_time_per_block();
        arm(node);
    }
    let b = node.beat_with_miner(miner, txs);
    if clocked(node) {
        node.chain.ctx.simulated_time = node.chain.ctx.simulated_time.max(b.header.timestamp);
        arm(node);
    }
    b
}

pub(super) fn evidence_template(node: &mut Rig, miner: MinerData, payload: Vec<u8>) -> MutableBlock {
    let txs = evidence_transactions(node, [payload]);
    heartbeat_with_transactions(node, miner, txs)
}

pub(super) fn evidence_transactions(
    node: &Rig,
    payloads: impl IntoIterator<Item = Vec<u8>>,
) -> Vec<kaspa_consensus_core::tx::Transaction> {
    use kaspa_consensus_core::{
        subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
        tx::{Transaction, TransactionInput, TransactionOutput},
    };
    let spendable = node.api().get_virtual_utxos(None, 1_000_000, false);
    let mut funds = (0..4).filter_map(|card| {
        let script = card_payout_spk(card);
        spendable
            .iter()
            .find(|(_, entry)| !entry.is_coinbase && entry.amount > 2_000_000 && entry.script_public_key == script)
            .map(|funding| (card, funding))
    });
    payloads
        .into_iter()
        .map(|payload| {
            let (card, (outpoint, utxo)) = funds.next().expect("unspent fixture fee float for the TB1 evidence carrier");
            let mut tx = Transaction::new(
                crate::constants::TX_VERSION,
                vec![TransactionInput::new(*outpoint, vec![], 0, 1)],
                vec![TransactionOutput::new(utxo.amount - 2_000_000, card_payout_spk(card))],
                0,
                SUBNETWORK_ID_PALW_LIFECYCLE,
                0,
                payload,
            );
            super::super::super::t12_round_lane_e2e::sign_spend(
                &mut tx,
                utxo.clone(),
                card,
                node.config.params.storage_mass_parameter,
            );
            tx
        })
        .collect()
}

pub(super) async fn arrive(node: &mut Rig, b: Block, label: &str) {
    if clocked(node) {
        node.chain.ctx.simulated_time = node.chain.ctx.simulated_time.max(b.header.timestamp);
        arm(node);
    }
    node.arrive(b, label).await;
}

pub(super) fn carrier(node: &mut Rig, card: usize, miner: MinerData) -> MutableBlock {
    if clocked(node) { heartbeat(node, miner) } else { node.attempt_with_miner(card, vec![], no_evm(), vec![], miner) }
}

pub(super) async fn next(node: &mut Rig, card: usize) -> Block {
    if clocked(node) {
        let b = heartbeat(node, new_miner_data());
        node.take(b, "heartbeat before EXEC release").await
    } else {
        extend(node, &[card]).await.remove(0)
    }
}

#[derive(Serialize, Deserialize)]
pub(super) struct Provision {
    pub roots: [Hash64; 4],
    // This holder's evaluations only. Dealer d's intercept and slope never enter the transport.
    shares: Vec<Vec<u8>>,
}

impl Provision {
    pub(super) fn holder_shares(&self) -> Vec<(usize, Tb1Field)> {
        self.shares.iter().enumerate().map(|(d, b)| (d, Tb1Field::from_bytes(b.as_slice().try_into().unwrap()).unwrap())).collect()
    }
}

fn random_coefficient() -> Tb1Field {
    loop {
        let mut bytes = [0; 66];
        OsRng.fill_bytes(&mut bytes);
        bytes[0] &= 1;
        if let Some(value) = Tb1Field::from_bytes(&bytes) {
            return value;
        }
    }
}

fn digest(id: &[u8; 64], dealer: usize, coefficients: &[Tb1Field]) -> Hash64 {
    let mut h = Sha512::new()
        .chain_update(b"MISAKA/test/TB1/dealer-polynomial/v1")
        .chain_update(id)
        .chain_update((dealer as u32).to_be_bytes());
    for c in coefficients {
        h.update(c.to_bytes());
    }
    Hash64::from_bytes(h.finalize().into())
}

pub(super) fn provision(dir: &Path, child: Option<&Path>, slot: usize, id: &[u8; 64]) -> Provision {
    let file = |p: &Path, slot: usize| p.join(format!("panel-dealer-input-{slot}.json"));
    if let Some(p) = child {
        return serde_json::from_reader(File::open(file(p, slot)).unwrap()).unwrap();
    }
    let polynomials: [[Tb1Field; 2]; 4] = std::array::from_fn(|_| [random_coefficient(), random_coefficient()]);
    let roots = std::array::from_fn(|d| digest(id, d, &polynomials[d]));
    let modulus = (Uint576::from_u64(1) << 521) - Uint576::from_u64(1);
    for holder in 0..3 {
        let x = Uint576::from_u64(holder as u64 + 1);
        let mut shares = Vec::new();
        for [a, b] in &polynomials[..3] {
            let value = (Uint576::from_be_bytes_var(&a.to_bytes()).unwrap() + x * Uint576::from_be_bytes_var(&b.to_bytes()).unwrap())
                % modulus;
            shares.push(value.to_be_bytes()[6..].to_vec());
        }
        serde_json::to_writer(File::create_new(file(dir, holder)).unwrap(), &Provision { roots, shares }).unwrap();
    }
    serde_json::from_reader(File::open(file(dir, slot)).unwrap()).unwrap()
}

// The holder keys and the global verifier keys have independent roles. Each
// verifier alone generates its session key; the pool exchange contains only public keys.
pub(super) struct Actors {
    mine: ml_dsa_65::MLDSA65KeyPair,
    keys: std::collections::BTreeMap<Hash64, ml_dsa_65::MLDSA65VerificationKey>,
    pub policy: LocalReplayPolicy,
}

impl Actors {
    pub(super) fn enroll(parent: &mut Option<UnixStream>, peers: &mut [(Child, UnixStream)]) -> Self {
        let mine = ml_dsa_65::generate_key_pair(rand::random());
        let public = mine.verification_key.as_slice().to_vec();
        let all = if let Some(s) = parent {
            send(s, &public);
            receive::<Vec<Vec<u8>>>(s)
        } else {
            let mut all = vec![public];
            for (_, s) in peers.iter_mut() {
                all.push(receive(s));
            }
            for (_, s) in peers.iter_mut() {
                send(s, &all);
            }
            all
        };
        let keys: std::collections::BTreeMap<_, _> = all
            .iter()
            .map(|bytes| {
                let vk = ml_dsa_65::MLDSA65VerificationKey::new(bytes.as_slice().try_into().unwrap());
                (actor_id(&vk), vk)
            })
            .collect();
        assert_eq!(keys.len(), 3);
        assert!(keys.contains_key(&actor_id(&mine.verification_key)));
        Self { mine, keys, policy: LocalReplayPolicy::new(2).unwrap() }
    }
    pub(super) fn candidates(&self) -> Vec<(Hash64, u64)> {
        self.keys.keys().copied().zip([1, 2, 4]).collect()
    }
}
async fn carry_to_daa<const N: usize>(
    node: &mut Rig,
    parent: &mut Option<UnixStream>,
    peers: &mut [(Child, UnixStream)],
    target: u64,
    marker: [u8; N],
) -> Block {
    let start = node.chain.daa_of(node.chain.sink());
    assert!(start < target);
    for _ in 0..=2 * (target - start) {
        let next_score = node.api().get_virtual_daa_score();
        let block = if next_score == target {
            local_carrier(node, parent, peers, 0, marker, "original PANEL deadline").await
        } else {
            local_carrier(node, parent, peers, 0, [], "waiting for original PANEL deadline").await
        };
        assert_eq!(block.header.daa_score, next_score);
        if block.header.daa_score == target {
            return block;
        }
    }
    panic!("PANEL test did not reach DAA {target}");
}

// Each selected verifier resolves the registered scales from its own signing store.
// No fallback to the instance used to prepare the Original before the draw.
fn replay_from_store(
    dir: &Path,
    available: bool,
    replay: impl FnOnce(&LocalInstance<'_>) -> std::io::Result<[u8; 64]>,
) -> std::io::Result<[u8; 64]> {
    use misaka_palw_sdk::qf1_local::residual_tiles::Candidate;
    use misaka_palw_tir_exec::node::TirArtifactV1;
    let section = dir.join("signer-registered-scales.section");
    if available {
        std::fs::hard_link(std::env::var_os("MISAKA_PRIVATE_QF1_TILES").unwrap(), &section)?;
    }
    File::open(&section)?;
    let parent =
        TirArtifactV1::open(Path::new(&std::env::var_os("MISAKA_PRIVATE_QF1_FULL").unwrap())).map_err(std::io::Error::other)?;
    let recipe = Candidate::over(&parent).map_err(std::io::Error::other)?;
    let asset = recipe.open(&parent, &section).map_err(std::io::Error::other)?;
    let instance = recipe.bind_full(&asset).map_err(std::io::Error::other)?;
    replay(&instance)
}

pub(super) fn reopen_checkpoint(config: &Config, path: &Path) -> TestConsensus {
    use kaspa_database::prelude::ConnBuilder;
    let db =
        ConnBuilder::default().with_files_limit(20).with_db_path(path.to_path_buf()).with_create_if_missing(false).build().unwrap();
    let mut resumed = config.clone();
    resumed.process_genesis = false;
    let (sender, _rx) = async_channel::unbounded();
    TestConsensus::with_db(db, &resumed, sender)
}

// Only public PANEL evidence and the two native database checkpoints cross
// the process boundary. Holder journals and signing keys stay in the writer.
#[derive(serde::Serialize, serde::Deserialize)]
struct ColdPanelReceipt {
    writer: u32,
    observed: Hash64,
    deadline_tip: Hash64,
    close: Hash64,
    batch: Hash64,
    certificate: Hash64,
    accepted: Hash64,
    marker: Hash64,
    close_marker: Hash64,
    domain: Hash64,
    deadline: u64,
}

#[derive(Serialize, Deserialize)]
struct ColdPanelSeed {
    keys: Vec<Vec<u8>>,
    transcript_roots: [Hash64; 4],
    ready: Vec<Vec<u8>>,
    consumers: Hash64,
    releases: Vec<ReleaseRecord>,
}

#[derive(Serialize, Deserialize)]
pub(super) struct IntakeRow {
    pub slot: u64,
    pub registration: Hash64,
    pub original: Hash64,
    pub block: Hash64,
    pub funding: Hash64,
    pub material: Hash64,
    pub deadline: u64,
}

#[derive(Serialize, Deserialize)]
pub(super) struct ColdPanelIntake {
    pub exec_inputs: Hash64,
    pub exec_close: Hash64,
    pub exec_certificate: Hash64,
    pub exec_seed: [u8; 32],
    pub rows: Vec<IntakeRow>,
    pub candidates: Vec<(Hash64, u64)>,
}

impl ColdPanelIntake {
    fn verify(&self, dir: &Path, meta: &ColdPanelReceipt, db: &TestConsensus, expected_root: Hash64, seed: &[u8; 32]) {
        use kaspa_consensus_core::palw_panel_v2::{
            palw_draw_key_cmp_v1, palw_draw_neg_log2_q64_v1, palw_draw_ticket_u64_v1, palw_panel_stake_ticket_v1,
        };
        use sha3::Sha3_512;
        let [row] = self.rows.as_slice() else { panic!("expected the private fixture's single submitted Original") };
        assert_eq!(self.candidates.len(), 3);
        let ancestor = |a, b| assert!(a != b && db.is_chain_ancestor_of(a, b).unwrap());
        ancestor(self.exec_close, self.exec_certificate);
        ancestor(self.exec_certificate, row.block);
        ancestor(row.block, meta.close);
        assert_eq!(row.deadline, meta.deadline);
        assert!(db.palw_coinbase_commits_to(meta.domain.as_bytes(), row.block, &row.original.as_bytes()).unwrap());
        let mut hash = Sha3_512::new()
            .chain_update(b"MISAKA/private/QF1/local-panel-inputs/v1")
            .chain_update(self.exec_inputs.as_bytes())
            .chain_update(self.exec_close.as_bytes())
            .chain_update(self.exec_certificate.as_bytes())
            .chain_update(self.exec_seed)
            .chain_update((self.rows.len() as u32).to_be_bytes());
        for r in &self.rows {
            hash.update(r.slot.to_be_bytes());
            for v in [r.registration, r.original, r.block, r.funding, r.material] {
                hash.update(v.as_bytes());
            }
            hash.update(r.deadline.to_be_bytes());
        }
        hash.update((self.candidates.len() as u32).to_be_bytes());
        assert!(self.candidates.windows(2).all(|w| w[0].0 < w[1].0));
        for (id, weight) in &self.candidates {
            assert!(*weight > 0);
            hash.update(id.as_bytes());
            hash.update(weight.to_be_bytes());
        }
        let root = Hash64::from_bytes(hash.finalize().into());
        assert_eq!(root, expected_root);
        let anchor = Hash64::from_bytes(
            Sha3_512::new()
                .chain_update(b"MISAKA/private/TB1/PANEL/draw-anchor/v1")
                .chain_update(meta.close_marker.as_bytes())
                .chain_update(seed)
                .finalize()
                .into(),
        );
        let mut ranked: Vec<_> = self
            .candidates
            .iter()
            .map(|&(id, weight)| {
                let ticket = palw_panel_stake_ticket_v1(anchor, &row.original, &id);
                (palw_draw_neg_log2_q64_v1(palw_draw_ticket_u64_v1(&ticket)), weight, id)
            })
            .collect();
        ranked.sort_by(|a, b| palw_draw_key_cmp_v1(a.0, a.1, &a.2, b.0, b.1, &b.2));
        let duty: [u8; 64] = Sha3_512::new()
            .chain_update(REPLAY_SIGNATURE_DOMAIN)
            .chain_update(meta.close_marker.as_bytes())
            .chain_update(row.original.as_bytes())
            .chain_update(row.block.as_bytes())
            .chain_update(meta.certificate.as_bytes())
            .chain_update(anchor.as_bytes())
            .finalize()
            .into();
        let replies =
            LocalReplayPolicy::new(2).unwrap().read_reply_evidence(File::open(dir.join("accepted-panel-replies")).unwrap()).unwrap();
        let mut commitment = Sha3_512::new().chain_update(b"MISAKA/private/TB1/PANEL/reply-set/v1").chain_update(duty);
        for ((public, sig), (_, _, expected)) in replies.iter().zip(&ranked) {
            assert_eq!(actor_id(public), *expected);
            let msg: [u8; 64] = Sha3_512::new().chain_update(duty).chain_update(expected.as_bytes()).finalize().into();
            ml_dsa_65::verify(public, &msg, REPLAY_SIGNATURE_DOMAIN, sig).unwrap();
            commitment.update(expected.as_bytes());
            commitment.update(sig.as_slice());
        }
        assert_eq!(Hash64::from_bytes(commitment.finalize().into()), meta.marker);
        println!(
            "QF1 recovered intake pid={} root={} original={} seats={},{}",
            std::process::id(),
            root,
            row.original,
            ranked[0].2,
            ranked[1].2
        );
    }
}

fn recover_cold_panel_seed(dir: &Path, meta: &ColdPanelReceipt, observation: &TestConsensus, expired: &TestConsensus) {
    use misaka_palw_remote::tb1_exposure::publication::NativePublicationGate;
    let path = dir.join("seed-evidence.json");
    let file = File::open(path).unwrap();
    assert!(file.metadata().unwrap().len() <= 512 * 1024);
    let saved: ColdPanelSeed = serde_json::from_reader(file).unwrap();
    assert_eq!((saved.keys.len(), saved.ready.len(), saved.releases.len()), (4, 9, 3));
    let public: Vec<_> = saved.keys.iter().map(|k| ml_dsa_65::MLDSA65VerificationKey::new(k.as_slice().try_into().unwrap())).collect();
    let coordinates: Vec<_> = (1..=4).map(field).collect();
    let roster = Tb1Roster { coordinates: &coordinates, holder_keys: &public, f: 1 };
    // The isolated unregistered fixture fixes these values before EXEC.
    let round = Tb1MiningRound {
        chain_id: meta.domain.as_bytes(),
        roster_root: roster.root().unwrap(),
        role_epoch: 1,
        logical_round: 3,
        purpose: Tb1MiningPurpose::Panel,
        schedule_id: [9; 64],
        close_blue_score: 25,
        release_from_daa: 26,
        deadline_exclusive_daa: 50,
    };
    let id = round.id();
    let q =
        qualification_from_ready(&id, roster, &[0, 1, 2], 29, Some(&saved.transcript_roots), |i, _| saved.ready[i].clone()).unwrap();
    let closure = Tb1MiningClosure {
        round,
        qual_root: q.root(),
        consumer_set_root: saved.consumers.as_bytes(),
        policy_root: LocalReplayPolicy::new(2).unwrap().id().as_bytes(),
    };
    let gate = NativePublicationGate::observe(observation.as_ref(), meta.close, &closure, &q).unwrap();
    let parent = observation.get_ghostdag_data(meta.close).unwrap().selected_parent;
    let freeze_id = closure.commitment(parent);
    assert_eq!(freeze_id, meta.close_marker.as_bytes());
    let decoded: Vec<_> = saved.releases.iter().map(ReleaseRecord::decode).collect();
    let rows = views(&decoded, Tb1ReleaseContext { round_ref: &id, qual_root: q.root(), freeze_id });
    let seed = gate.read_seed(expired.as_ref(), meta.batch, meta.certificate, &rows, &q, LIMITS).unwrap();
    assert!(gate.read_seed(expired.as_ref(), meta.batch, meta.certificate, &rows[..2], &q, LIMITS).is_err());
    if std::env::var_os("MISAKA_PRIVATE_COLD_PANEL_INTAKE").is_some() {
        let file = File::open(dir.join("intake.json")).unwrap();
        assert!(file.metadata().unwrap().len() <= 64 * 1024);
        let intake: ColdPanelIntake = serde_json::from_reader(file).unwrap();
        intake.verify(dir, meta, expired, saved.consumers, seed.as_bytes());
    }
    for dealer in 0..3 {
        let points: Vec<_> = decoded.iter().map(|r| Tb1DealerPoint { holder_slot: r.holder, value: r.shares[dealer].1 }).collect();
        let polynomial = reconstruct_dealer(&coordinates, 1, &points, LIMITS.matrix_cells).unwrap();
        assert_eq!(digest(&id, dealer, &polynomial.coefficients), saved.transcript_roots[dealer]);
    }
    println!(
        "QF1 restored seed pid={} qualification={} seed={}",
        std::process::id(),
        Hash64::from_bytes(q.root()),
        faster_hex::hex_string(seed.as_bytes())
    );
}

#[tokio::test]
#[ignore = "requires the two completed native checkpoints of a private QF1 peer"]
async fn private_read_cold_panel_receipt() {
    let dir = std::path::PathBuf::from(std::env::var_os("MISAKA_PRIVATE_COLD_PANEL_DIR").unwrap());
    let meta: ColdPanelReceipt = serde_json::from_slice(&std::fs::read(dir.join("receipt.json")).unwrap()).unwrap();
    assert_ne!(meta.writer, std::process::id());
    let config = parts(Some(FENCE)).0;
    let observation = reopen_checkpoint(&config, &dir.join("before-panel-release"));
    assert_eq!(observation.get_sink(), meta.observed);
    let snapshot = observation.get_native_settlement_snapshot().unwrap().unwrap();
    assert_eq!(snapshot.generation, meta.observed);
    assert!(observation.is_chain_ancestor_of(meta.close, snapshot.finalized.unwrap()).unwrap());
    assert!(observation.palw_coinbase_commits_to(meta.domain.as_bytes(), meta.close, &meta.close_marker.as_bytes()).unwrap());
    let expired = reopen_checkpoint(&config, &dir.join("at-original-deadline"));
    assert_eq!(expired.get_sink(), meta.deadline_tip);
    let current = expired.get_native_settlement_snapshot().unwrap().unwrap();
    assert_eq!(current.generation, meta.deadline_tip);
    assert!(expired.is_chain_ancestor_of(meta.close, current.finalized.unwrap()).unwrap());
    for (ancestor, child) in [
        (meta.close, meta.observed),
        (meta.observed, meta.batch),
        (meta.batch, meta.certificate),
        (meta.certificate, meta.accepted),
        (meta.accepted, meta.deadline_tip),
    ] {
        assert_ne!(ancestor, child);
        assert!(expired.is_chain_ancestor_of(ancestor, child).unwrap());
    }
    for block in [meta.accepted, meta.deadline_tip] {
        assert!(expired.palw_coinbase_commits_to(meta.domain.as_bytes(), block, &meta.marker.as_bytes()).unwrap());
    }
    assert!(expired.get_header(meta.accepted).unwrap().daa_score < meta.deadline);
    assert_eq!(expired.get_header(meta.deadline_tip).unwrap().daa_score, meta.deadline);
    if std::env::var_os("MISAKA_PRIVATE_COLD_PANEL_SEED").is_some() {
        recover_cold_panel_seed(&dir, &meta, &observation, &expired);
    }
    let path = dir.join("accepted-panel-replies");
    assert_eq!(LocalReplayPolicy::new(2).unwrap().read_reply_evidence(File::open(&path).unwrap()).unwrap().len(), 2);
    println!(
        "QF1 cold reader pid={} receipt={} replies={}",
        std::process::id(),
        serde_json::to_string(&meta).unwrap(),
        faster_hex::hex_string(&std::fs::read(path).unwrap())
    );
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn release<'t>(
    node: &mut Rig,
    parent: &mut Option<UnixStream>,
    peers: &mut [(Child, UnixStream)],
    dir: &Path,
    slot: usize,
    key: &ml_dsa_65::MLDSA65KeyPair,
    q: &Tb1Qualification<'_>,
    id: &[u8; 64],
    panel_ready: &[Vec<u8>],
    cut: &LocalPanelCut,
    close_parent: Hash64,
    original_receipt: Hash64,
    close: &Block,
    release_from_daa: u64,
    setup: &Provision,
    exec_seed: &[u8; 32],
    exec_rows: &[PublishedRelease<'_>],
    actors: &Actors,
    input: &DerivedExecInput<'t>,
    recover_exec: impl Fn(&dyn ConsensusApi) -> DerivedExecInput<'t>,
    recover_branch_exec: impl Fn(&dyn ConsensusApi, Hash64, &NativeSettlementSnapshotV1) -> BranchExecInput<'t>,
    instance: &LocalInstance<'_>,
    metadata_fixture: Option<(&misaka_palw_tir_exec::node::TirArtifactV1, u16)>,
    body: &LocalOriginalBody,
    owner: &ml_dsa_65::MLDSA65Signature,
    original: Hash64,
) {
    let marker = cut.commitment(close_parent);
    let context = Tb1ReleaseContext { round_ref: id, qual_root: q.root(), freeze_id: marker };
    let shares = setup.holder_shares();
    let row = Tb1HolderRelease { context, holder_slot: slot, shares: &shares };
    Tb1ExposureJournal::provision(dir, id, &key.verification_key, slot as u32).unwrap();
    let mut journal = Tb1ExposureJournal::resume(dir, id, &key.verification_key, slot as u32).unwrap();
    let freeze = cut.resolve(node.api().as_ref(), close.hash(), q).unwrap();
    assert!(journal.reserve_signing_message(node.api().as_ref(), freeze, q, &row, 2048).is_err());

    let _later = local_carrier(node, parent, peers, 0, [81; 64], "PANEL confirmation heartbeat").await;
    plant_frontier(node, close.header.blue_score, close.hash());
    let work = local_carrier(node, parent, peers, 0, [82; 64], "independent mature work fixture").await;
    let fact = MatureUsefulWorkV1 {
        identity: Hash64::from_u64_word(0x1301),
        anchor: work.hash(),
        operator: Hash64::from_u64_word(0x1302),
        class: Hash64::from_u64_word(0x1303),
        anchor_blue: work.header.blue_score,
        accepted_blue: work.header.blue_score,
        anchor_daa: work.header.daa_score,
        accepted_daa: work.header.daa_score,
        matured_daa: work.header.daa_score,
        work: 10,
    };
    node.vp().native_fact_override.lock().entry(work.hash()).or_default().push(fact);
    node.vp().native_readiness_memo.lock().take();
    node.vp().pruning_point_store.write().set(close.hash(), 2).unwrap();
    let stable = local_carrier(node, parent, peers, 0, [83; 64], "PANEL stability observation").await;
    assert_eq!(snapshot_of(node).unwrap().finalized, Some(close.hash()), "{}", serde_json::to_string(&readiness_of(node)).unwrap());
    let mut n = 0;
    while node.chain.daa_of(node.chain.sink()) < release_from_daa {
        assert!(n < 240, "PANEL release DAA stalled at {}", node.chain.daa_of(node.chain.sink()));
        local_carrier(node, parent, peers, 0, [84; 64], "wait for PANEL DAA").await;
        n += 1;
    }
    let release_daa = node.chain.daa_of(node.chain.sink());
    let gate = cut.observe(node.api().as_ref(), close.hash(), q).unwrap();
    let checkpoint_replay = std::env::var_os("MISAKA_PRIVATE_QF1_CHECKPOINT_REPLAY").is_some();
    let observed_at = node.chain.sink();
    let archive_dir =
        std::env::var_os("MISAKA_PRIVATE_COLD_PANEL_WRITE").map(|root| std::path::PathBuf::from(root).join(slot.to_string()));
    if let Some(path) = &archive_dir {
        assert!(checkpoint_replay);
        std::fs::create_dir(path).unwrap();
    }
    let checkpoint_dir = archive_dir.as_deref().unwrap_or(dir);
    if checkpoint_replay || std::env::var_os("MISAKA_PRIVATE_QF1_MISSING_REPLAY_MATERIAL").is_some() {
        node.api().checkpoint_database(&checkpoint_dir.join("before-panel-release"));
    }
    let msg = journal.reserve_signing_message(node.api().as_ref(), gate.holder_freeze(), q, &row, 2048).unwrap();
    let sig = ml_dsa_65::sign(&key.signing_key, &msg, RELEASE_CONTEXT, rand::random()).unwrap();
    let record = ReleaseRecord {
        holder: slot,
        shares: shares.iter().map(|(d, v)| (*d, v.to_bytes().to_vec())).collect(),
        signature: sig.as_slice().to_vec(),
    };
    let records = if let Some(s) = parent {
        send(s, &record);
        receive::<Vec<ReleaseRecord>>(s)
    } else {
        let mut records = vec![record];
        for (_, s) in peers.iter_mut() {
            records.push(receive(s));
        }
        for (_, s) in peers.iter_mut() {
            send(s, &records);
        }
        records
    };
    let decoded: Vec<_> = records.iter().map(ReleaseRecord::decode).collect();
    let rows = views(&decoded, context);
    let batch = local_evidence_carrier(
        node,
        parent,
        peers,
        0,
        batch_commitment(&rows, LIMITS).unwrap(),
        release_batch_payload(&rows, LIMITS).unwrap(),
        "signed PANEL shares",
    )
    .await;
    let api = node.api().as_ref();
    assert!(gate.propose(api, batch.hash(), exec_rows, q, LIMITS).is_err());
    let certificate_marker = gate.propose_published(api, batch.hash(), q, LIMITS).unwrap();
    let certificate_parent = snapshot_of(node).unwrap();
    assert_eq!(certificate_parent.generation, batch.hash());
    let cert = local_carrier(node, parent, peers, 0, certificate_marker, "PANEL seed certificate").await;
    let api = node.api().as_ref();
    let seed = gate.read_published_seed(api, batch.hash(), cert.hash(), q, LIMITS).unwrap();
    assert_eq!(seed, gate.read_seed(api, batch.hash(), cert.hash(), &rows, q, LIMITS).unwrap());
    assert!(gate.read_seed(api, batch.hash(), cert.hash(), &rows[..2], q, LIMITS).is_err());
    assert_ne!(seed.as_bytes(), exec_seed);
    if let Some(path) = &archive_dir {
        let saved = ColdPanelSeed {
            keys: q.roster().holder_keys.iter().map(|k| k.as_slice().to_vec()).collect(),
            transcript_roots: setup.roots,
            ready: panel_ready.to_vec(),
            consumers: Hash64::from_bytes(cut.consumer_set_root()),
            releases: records,
        };
        std::fs::write(path.join("seed-evidence.json"), serde_json::to_vec(&saved).unwrap()).unwrap();
    }
    for d in 0..3 {
        let points: Vec<_> = decoded.iter().map(|r| Tb1DealerPoint { holder_slot: r.holder, value: r.shares[d].1 }).collect();
        let recovered = reconstruct_dealer(q.roster().coordinates, 1, &points, LIMITS.matrix_cells).unwrap();
        assert_eq!(digest(id, d, &recovered.coefficients), setup.roots[d]);
    }
    peer_log!(
        "TB1 stable PANEL pid={} slot={} close={} stable={} stable_daa={} release_daa={} batch={} certificate={} seed={}",
        std::process::id(),
        slot,
        close.hash(),
        stable.hash(),
        stable.header.daa_score,
        release_daa,
        batch.hash(),
        cert.hash(),
        faster_hex::hex_string(seed.as_bytes())
    );
    if let Some(s) = parent {
        send(s, seed.as_bytes());
    } else {
        for (_, s) in peers.iter_mut() {
            assert_eq!(&receive::<[u8; 32]>(s), seed.as_bytes());
        }
    }
    let api = node.api().as_ref();
    let draw =
        cut.draw_published_replays(api, close.hash(), &gate, batch.hash(), cert.hash(), q, LIMITS, original, actors.policy).unwrap();
    let history = CandidateHistory { api, candidate: Some(cert.hash()) };
    let candidate_draw = cut
        .draw_replays_in_candidate(
            &history,
            close.hash(),
            batch.hash(),
            &certificate_parent,
            &gate,
            batch.hash(),
            &cert,
            q,
            LIMITS,
            original,
            actors.policy,
        )
        .unwrap();
    assert_eq!(candidate_draw, draw.replay_duty());
    assert!(
        cut.draw_replays(
            api,
            close.hash(),
            &gate,
            batch.hash(),
            cert.hash(),
            &rows,
            q,
            LIMITS,
            original,
            LocalReplayPolicy::new(1).unwrap()
        )
        .is_err()
    );
    let key = &actors.mine.verification_key;
    let actor = actor_id(key);
    let selected = draw.selected().contains(&actor);
    let missing_trial = std::env::var_os("MISAKA_PRIVATE_QF1_MISSING_REPLAY_MATERIAL").is_some();
    let normal_trial = std::env::var_os("MISAKA_PRIVATE_QF1_NO_DISPUTE").is_some();
    assert!(!(missing_trial && normal_trial) && (!(missing_trial || normal_trial) || !inconsistent_readout_fixture()));
    let unavailable = missing_trial && draw.selected()[0] == actor;
    let replay = |model: &LocalInstance<'_>| draw.replay_published_message(api, input, model, key);
    let message = if missing_trial && selected { replay_from_store(dir, !unavailable, replay) } else { replay(instance) };
    let before_stop = if inconsistent_readout_fixture() {
        let reason = message.unwrap_err().to_string();
        if selected {
            assert!(reason.contains("different replayed local QF1 Original"), "{reason}");
        } else {
            assert!(reason.contains("not selected"), "{reason}");
        }
        assert!(draw.reply_commitment(api, &[]).is_err());
        peer_log!(
            "TB1 replay duty pid={} actor={} replayed={} selected={:?} original={} replies=none",
            std::process::id(),
            actor,
            selected,
            draw.selected(),
            original
        );
        cert.hash()
    } else {
        let reply = if unavailable {
            assert_eq!(message.unwrap_err().kind(), std::io::ErrorKind::NotFound);
            None
        } else if selected {
            Some((
                actor,
                ml_dsa_65::sign(&actors.mine.signing_key, &message.unwrap(), REPLAY_SIGNATURE_DOMAIN, rand::random())
                    .unwrap()
                    .as_slice()
                    .to_vec(),
            ))
        } else {
            assert!(message.unwrap_err().to_string().contains("not selected"));
            None
        };
        let ordered = if let Some(s) = parent {
            send(s, &reply);
            receive::<Vec<(Hash64, Vec<u8>)>>(s)
        } else {
            let mut votes = std::collections::BTreeMap::new();
            for opt in std::iter::once(reply).chain(peers.iter_mut().map(|(_, s)| receive(s))) {
                if let Some((id, signature)) = opt {
                    assert!(votes.insert(id, signature).is_none());
                }
            }
            let ordered: Vec<_> = draw.selected().iter().filter_map(|id| votes.remove(id).map(|sig| (*id, sig))).collect();
            assert!(votes.is_empty());
            for (_, s) in peers.iter_mut() {
                send(s, &ordered);
            }
            ordered
        };
        let sigs: Vec<_> = ordered.iter().map(|(_, b)| ml_dsa_65::MLDSA65Signature::new(b.as_slice().try_into().unwrap())).collect();
        let replies: Vec<_> = ordered.iter().zip(&sigs).map(|((id, _), sig)| (&actors.keys[id], sig)).collect();
        if missing_trial {
            use std::io::ErrorKind;
            assert_eq!(ordered.len(), 1);
            assert_eq!(ordered[0].0, draw.selected()[1]);
            assert!(draw.verify_reply(api, replies[0].0, replies[0].1).is_ok());
            assert_eq!(draw.reply_commitment(api, &[replies[0], replies[0]]).unwrap_err().kind(), ErrorKind::InvalidData);
            assert_eq!(
                draw.reply_commitment(api, &[(&actors.keys[&draw.selected()[0]], replies[0].1)]).unwrap_err().kind(),
                ErrorKind::InvalidData
            );
            let deadline = input.terms().deadline_exclusive_daa;
            let early = carry_to_daa(node, parent, peers, deadline - 1, []).await;
            assert_eq!(
                draw.unverified_expiry_marker(node.api().as_ref(), early.hash(), 128).unwrap_err().kind(),
                ErrorKind::WouldBlock
            );
            assert_eq!(draw.reply_commitment(node.api().as_ref(), &replies).unwrap_err().kind(), ErrorKind::WouldBlock);
            let boundary = carry_to_daa(node, parent, peers, deadline, []).await;
            let api = node.api().as_ref();
            assert_eq!(draw.reply_commitment(api, &replies).unwrap_err().kind(), ErrorKind::TimedOut);
            if selected {
                assert_eq!(draw.replay_message(api, input, instance, body, owner, key).unwrap_err().kind(), ErrorKind::TimedOut);
            }
            peer_log!(
                "QF1 missing material pid={} actor={} original={} certificate={} selected={:?} missing={} signed={} signatures={} pending_daa={} deadline={} tip={}",
                std::process::id(),
                actor,
                original,
                cert.hash(),
                draw.selected(),
                unavailable,
                selected && !unavailable,
                replies.len(),
                deadline - 1,
                deadline,
                node.chain.sink()
            );
            assert!(draw.unverified_expiry_marker(api, early.hash(), 128).is_err());
            let budget = draw.unverified_expiry_marker(api, boundary.hash(), 1).unwrap_err();
            assert!(matches!(
                budget.get_ref().and_then(|error| error.downcast_ref::<Tb1HistoryError>()),
                Some(Tb1HistoryError::ScanBudgetExceeded { component: "panel_deadline", limit: 1, .. })
            ));
            let marker = draw.unverified_expiry_marker(api, boundary.hash(), 128).unwrap();
            assert!(draw.read_unverified_expiry(api, boundary.hash(), boundary.hash(), 128).is_err());
            let terminal = local_carrier(node, parent, peers, 0, marker, "unverified PANEL expiry").await;
            draw.read_unverified_expiry(node.api().as_ref(), boundary.hash(), terminal.hash(), 128).unwrap();
            let path = checkpoint_dir.join("at-unverified-terminal");
            node.api().checkpoint_database(&path);
            let recovered = reopen_checkpoint(&node.config, &path);
            assert_eq!(recovered.get_sink(), terminal.hash());
            let before = reopen_checkpoint(&node.config, &checkpoint_dir.join("before-panel-release"));
            let restored_gate = cut.observe(before.as_ref(), close.hash(), q).unwrap();
            drop(before);
            let restored = cut
                .recorded_draw(
                    recovered.as_ref(),
                    close.hash(),
                    &restored_gate,
                    batch.hash(),
                    cert.hash(),
                    &rows,
                    q,
                    LIMITS,
                    original,
                    actors.policy,
                )
                .unwrap();
            restored.read_unverified_expiry(recovered.as_ref(), boundary.hash(), terminal.hash(), 128).unwrap();
            peer_log!(
                "QF1 unverified expiry pid={} original={} boundary={} terminal={} marker={}",
                std::process::id(),
                original,
                boundary.hash(),
                terminal.hash(),
                Hash64::from_bytes(marker)
            );
            return;
        }
        assert_eq!(ordered.len(), draw.selected().len());
        assert!(draw.reply_commitment(api, &replies[..1]).is_err());
        assert!(draw.reply_commitment(api, &[replies[0], replies[0]]).is_err());
        assert!(draw.reply_commitment(api, &[replies[1], replies[0]]).is_err());
        assert!(draw.verify_reply(api, replies[0].0, replies[1].1).is_err());
        let (commitment, payload) = draw.publish_replies(api, &replies).unwrap();
        let before_replies = api.get_native_settlement_snapshot().unwrap().unwrap();
        assert!(draw.read_replies(api, cert.hash(), &replies).is_err());
        let accepted =
            local_evidence_carrier(node, parent, peers, 0, commitment, payload.clone(), "selected full replay replies").await;
        let approval = draw.read_published_replies(node.api().as_ref(), accepted.hash()).unwrap();
        assert_eq!(
            (approval.original(), approval.accepting_block(), approval.selected_parent()),
            (original, accepted.hash(), accepted.hash())
        );
        use kaspa_consensus_core::palw_tb1::panel_reply_carrier::{
            PanelReplyCarrierError, decode_panel_replies, panel_replies_from_block,
        };
        assert!(matches!(decode_panel_replies(&payload[0..payload.len() - 1], replies.len()), Err(PanelReplyCarrierError::Encoding)));
        assert!(matches!(decode_panel_replies(&payload, replies.len() + 1), Err(PanelReplyCarrierError::Encoding)));
        let transaction = accepted.transactions.iter().find(|tx| tx.payload == payload).unwrap();
        let mut duplicates = accepted.transactions.to_vec();
        duplicates.push(transaction.clone());
        assert!(matches!(
            panel_replies_from_block(&Block::new((*accepted.header).clone(), duplicates), replies.len()),
            Err(PanelReplyCarrierError::Multiple)
        ));
        peer_log!(
            "TB1 replay duty pid={} actor={} replayed={} selected={:?} original={} replies={}",
            std::process::id(),
            actor,
            selected,
            draw.selected(),
            original,
            accepted.hash()
        );
        if normal_trial {
            let evidence = checkpoint_dir.join("accepted-panel-replies");
            let mut file = File::create_new(&evidence).unwrap();
            for (public, signature) in &replies {
                std::io::Write::write_all(&mut file, public.as_slice()).unwrap();
                std::io::Write::write_all(&mut file, signature.as_slice()).unwrap();
            }
            file.sync_all().unwrap();
            drop(file);
            let accepted_settlement = node.api().get_native_settlement_snapshot().unwrap().unwrap();
            let deadline = input.terms().deadline_exclusive_daa;
            carry_to_daa(node, parent, peers, deadline - 1, []).await;
            let late = carry_to_daa(node, parent, peers, deadline, commitment).await;
            let api = node.api().as_ref();
            let unresolved = draw.unverified_expiry_marker(api, late.hash(), 128).unwrap_err();
            assert_eq!(unresolved.kind(), std::io::ErrorKind::WouldBlock);
            assert_eq!(
                unresolved.get_ref().and_then(|error| error.downcast_ref::<Tb1HistoryError>()),
                Some(&Tb1HistoryError::UnresolvedCommitment { block: accepted.hash() })
            );
            peer_log!("QF1 expiry withheld pid={} boundary={} pending={}", std::process::id(), late.hash(), accepted.hash());
            draw.read_replies(api, accepted.hash(), &replies).unwrap();
            let recorded = |block, signatures| {
                cut.read_recorded_replies(
                    api,
                    close.hash(),
                    &gate,
                    batch.hash(),
                    cert.hash(),
                    &rows,
                    q,
                    LIMITS,
                    original,
                    actors.policy,
                    block,
                    signatures,
                )
            };
            let recovered = actors.policy.read_reply_evidence(File::open(&evidence).unwrap()).unwrap();
            let recovered: Vec<_> = recovered.iter().map(|(key, sig)| (key, sig)).collect();
            recorded(accepted.hash(), recovered.as_slice()).unwrap();
            let deadline_checkpoint = checkpoint_dir.join("at-original-deadline");
            node.api().checkpoint_database(&deadline_checkpoint);
            let expired = reopen_checkpoint(&node.config, &deadline_checkpoint);
            assert_eq!(expired.get_sink(), late.hash());
            let late_settlement = expired.get_native_settlement_snapshot().unwrap().unwrap();
            let branch = CandidateHistory { api: expired.as_ref(), candidate: None };
            let published_at = |parent, state, block, claim| {
                cut.read_published_approval_at_parent(
                    &branch,
                    parent,
                    state,
                    close.hash(),
                    &gate,
                    batch.hash(),
                    cert.hash(),
                    q,
                    LIMITS,
                    claim,
                    actors.policy,
                    block,
                )
            };
            let archived = published_at(late.hash(), &late_settlement, accepted.hash(), original).unwrap();
            assert_eq!(
                (archived.original(), archived.accepting_block(), archived.selected_parent()),
                (original, accepted.hash(), late.hash())
            );
            assert!(published_at(cert.hash(), &before_replies, accepted.hash(), original).is_err());
            assert!(published_at(late.hash(), &late_settlement, accepted.hash(), Hash64::default()).is_err());
            let branch_input = recover_branch_exec(&branch, late.hash(), &late_settlement);
            let approved = branch_input.read_panel_approved_qf1_execution(&branch, archived, instance).unwrap();
            assert_eq!(approved.approval(), archived);
            let execution = approved.execution();
            assert_eq!(
                (execution.original(), execution.original_block(), execution.parent()),
                (original, original_receipt, late.hash())
            );
            assert_eq!(execution.attempt(), input.attempt());
            assert_eq!(execution.work(), instance.computational_work_key(input.tokens(), input.terms().decode_tokens).unwrap());
            let earlier_input = recover_branch_exec(&branch, cert.hash(), &before_replies);
            if let Some((artifact, final_norm)) = metadata_fixture {
                use misaka_palw_tir_exec::node::TirParamOpenerV1;
                let class = artifact.class().unwrap();
                let metadata = super::prediction::readout_metadata(final_norm, b"unit scale");
                let opening = artifact.param_opening(0).unwrap();
                let read = |class, metadata, opening| {
                    branch_input.read_panel_approved_qf1_execution_with_metadata(&branch, archived, class, metadata, opening)
                };
                let model_free = read(&class, &metadata, &opening).unwrap();
                assert_eq!(model_free.approval(), archived);
                assert_eq!(model_free.execution().work(), execution.work());
                assert_eq!(model_free.execution().attempt(), execution.attempt());
                let mut changed = metadata.clone();
                changed.scales[0] = Hash64::default();
                assert_eq!(
                    read(&class, &changed, &opening).err().unwrap().to_string(),
                    "QF1 Readout differs from the owner-signed descriptor"
                );
                let mut changed = class.clone();
                changed.layout.checkpoint_interval += 1;
                assert_eq!(
                    read(&changed, &metadata, &opening).err().unwrap().to_string(),
                    "QF1 metadata differs from the owner-signed class or tokenizer"
                );
                let mut changed = opening.clone();
                changed.operand.bytes[0] ^= 1;
                assert!(read(&class, &metadata, &changed).is_err());
                let mut changed = opening.clone();
                changed.leaf_count += 1;
                assert!(read(&class, &metadata, &changed).is_err());
                assert!(
                    earlier_input
                        .read_panel_approved_qf1_execution_with_metadata(&branch, archived, &class, &metadata, &opening)
                        .is_err()
                );
                peer_log!("QF1 public metadata work key pid={} key={}", std::process::id(), model_free.execution().work());
            }
            let terminal = branch_input.read_terminal_court_at_parent(&branch, original, original_receipt, 128).unwrap();
            assert!(!terminal.blocks_success());
            assert!(terminal.cases().is_empty());
            let (_, funding_parent) = expired.virtual_processor().palw_observer_state_at_cached_v1(late.hash()).unwrap();
            let unfunded = kaspa_consensus_core::palw_model_market_v1::moe2::award_reference::Moe2AwardReference {
                claim: original,
                attempt: *execution.attempt(),
                close: Hash64::default(),
                slot: 9,
                reservation: Hash64::default(),
            };
            assert_eq!(
                approved
                    .bind_moe2_funding(&branch, &unfunded, &funding_parent, &terminal, late.header.daa_score)
                    .unwrap_err()
                    .to_string(),
                "approved EXEC input carries no signed MOE2 quote"
            );
            let accepted_input = recover_branch_exec(&branch, accepted.hash(), &accepted_settlement);
            let accepted_approval = published_at(accepted.hash(), &accepted_settlement, accepted.hash(), original).unwrap();
            let accepted_execution = accepted_input.read_panel_approved_qf1_execution(&branch, accepted_approval, instance).unwrap();
            let still_open = accepted_input.read_terminal_court_at_parent(&branch, original, original_receipt, 128).unwrap();
            let (_, accepted_state) = expired.virtual_processor().palw_observer_state_at_cached_v1(accepted.hash()).unwrap();
            assert_eq!(
                accepted_execution
                    .bind_moe2_funding(&branch, &unfunded, &accepted_state, &still_open, accepted.header.daa_score)
                    .unwrap_err()
                    .to_string(),
                "terminal Court does not permit this MOE2 award",
            );
            assert_eq!(
                approved
                    .bind_moe2_funding(&branch, &unfunded, &funding_parent, &still_open, late.header.daa_score)
                    .unwrap_err()
                    .to_string(),
                "terminal Court belongs to a different selected parent or Original",
            );
            assert_eq!(
                earlier_input.read_panel_approved_qf1_execution(&branch, archived, instance).err().unwrap().to_string(),
                "PANEL approval belongs to a different selected parent"
            );
            assert_eq!(
                published_at(late.hash(), &late_settlement, late.hash(), original).unwrap_err().kind(),
                std::io::ErrorKind::TimedOut
            );
            peer_log!(
                "QF1 archived PANEL approval pid={} parent={} reply={} original={} work={}",
                std::process::id(),
                archived.selected_parent(),
                archived.accepting_block(),
                archived.original(),
                execution.work()
            );
            if checkpoint_replay {
                let observed = reopen_checkpoint(&node.config, &checkpoint_dir.join("before-panel-release"));
                assert_eq!(observed.get_sink(), observed_at);
                let restored_gate = cut.observe(observed.as_ref(), close.hash(), q).unwrap();
                drop(observed);
                let read = |block| {
                    cut.read_recorded_replies(
                        expired.as_ref(),
                        close.hash(),
                        &restored_gate,
                        batch.hash(),
                        cert.hash(),
                        &rows,
                        q,
                        LIMITS,
                        original,
                        actors.policy,
                        block,
                        &recovered,
                    )
                };
                read(accepted.hash()).unwrap();
                assert_eq!(read(late.hash()).unwrap_err().kind(), std::io::ErrorKind::TimedOut);
                assert!(cut.observe(expired.as_ref(), close.hash(), q).is_err());
                if let Some(path) = &archive_dir {
                    let record = ColdPanelReceipt {
                        writer: std::process::id(),
                        observed: observed_at,
                        deadline_tip: late.hash(),
                        close: close.hash(),
                        batch: batch.hash(),
                        certificate: cert.hash(),
                        accepted: accepted.hash(),
                        marker: Hash64::from_bytes(commitment),
                        close_marker: Hash64::from_bytes(marker),
                        domain: node.domain,
                        deadline,
                    };
                    std::fs::write(path.join("receipt.json"), serde_json::to_vec(&record).unwrap()).unwrap();
                }
                peer_log!(
                    "QF1 reopened checkpoints pid={} observed={} deadline_tip={} accepted={}",
                    std::process::id(),
                    observed_at,
                    late.hash(),
                    accepted.hash()
                );
            }
            let bytes = std::fs::read(&evidence).unwrap();
            assert!(actors.policy.read_reply_evidence(&bytes[..bytes.len() - 1]).is_err());
            assert!(actors.policy.read_reply_evidence(std::io::Read::chain(bytes.as_slice(), &[0u8][..])).is_err());
            peer_log!(
                "QF1 public replies pid={} block={} marker={} data={}",
                std::process::id(),
                accepted.hash(),
                faster_hex::hex_string(&commitment),
                faster_hex::hex_string(&bytes)
            );
            assert!(recorded(accepted.hash(), &replies[..1]).is_err());
            assert_eq!(recorded(late.hash(), replies.as_slice()).unwrap_err().kind(), std::io::ErrorKind::TimedOut);
            assert!(
                cut.draw_replays(api, close.hash(), &gate, batch.hash(), cert.hash(), &rows, q, LIMITS, original, actors.policy)
                    .is_err()
            );
            peer_log!(
                "QF1 historical replies pid={} original={} accepted={} late={}",
                std::process::id(),
                original,
                accepted.hash(),
                late.hash()
            );
            assert_eq!(draw.read_replies(api, late.hash(), &replies).unwrap_err().kind(), std::io::ErrorKind::TimedOut);
            assert_eq!(draw.reply_commitment(api, &replies).unwrap_err().kind(), std::io::ErrorKind::TimedOut);
            peer_log!(
                "QF1 undisputed pid={} original={} accepted={} accepted_daa={} deadline={} late={}",
                std::process::id(),
                original,
                accepted.hash(),
                accepted.header.daa_score,
                deadline,
                late.hash()
            );
            return;
        }
        accepted.hash()
    };
    use kaspa_consensus_core::palw_tb1::qf1_terminal_carrier::{Qf1TerminalCarrier, Qf1TerminalRelation};
    use misaka_palw_remote::tb1_exposure::exec_input::stop_case::{StopOutcome, StopWitness};
    let disclosure =
        |relation, disclosure| Qf1TerminalCarrier { relation, original_block: original_receipt, disclosure }.encode().unwrap();
    let wire = std::fs::read(dir.join(format!("{original}.stop"))).expect("Stop disclosure published with the Original");
    let api = node.api().as_ref();
    let witness = StopWitness::decode(body, &wire).unwrap();
    let mut changed = witness.clone();
    changed.stop[17] ^= 1;
    assert!(input.prepare_stop_case(api, original_receipt, body, owner, &changed).is_err());
    let case = input.prepare_stop_case(api, original_receipt, body, owner, &witness).unwrap();
    assert!(case.verdict_marker(api, before_stop).is_err());
    let stop_case = local_evidence_carrier(
        node,
        parent,
        peers,
        0,
        case.marker(),
        disclosure(Qf1TerminalRelation::Stop, &wire),
        "QF1 last answer / Stop case",
    )
    .await;
    let marker = case.verdict_marker(node.api().as_ref(), stop_case.hash()).unwrap();
    assert!(case.read_verdict(node.api().as_ref(), stop_case.hash(), stop_case.hash()).is_err());
    let stop_verdict = local_carrier(node, parent, peers, 0, marker, "QF1 last answer / Stop verdict").await;
    let outcome = case.read_verdict(node.api().as_ref(), stop_case.hash(), stop_verdict.hash()).unwrap();
    assert_eq!(outcome, StopOutcome::ChallengeNotEstablished);
    peer_log!(
        "TB1 Court Stop pid={} original={} case={} verdict={} result={:?} witness={}",
        std::process::id(),
        original,
        stop_case.hash(),
        stop_verdict.hash(),
        outcome,
        faster_hex::hex_string(&wire)
    );
    let before_end = node.api().get_native_settlement_snapshot().unwrap().unwrap();
    let republished = local_evidence_carrier(
        node,
        parent,
        peers,
        0,
        original.as_bytes(),
        body.publication_payload(owner),
        "Republished QF1 Original",
    )
    .await;
    let end_wire = std::fs::read(dir.join(format!("{original}.end"))).expect("terminal Readout and End published with the Original");
    let case = input.prepare_end_case(node.api().as_ref(), original_receipt, body, owner, &end_wire).unwrap();
    assert!(case.verdict_marker(node.api().as_ref(), stop_case.hash()).is_err());
    let opened = local_evidence_carrier(
        node,
        parent,
        peers,
        0,
        case.marker(),
        disclosure(Qf1TerminalRelation::End, &end_wire),
        "QF1 Readout / End case",
    )
    .await;
    let at_case = node.api().get_native_settlement_snapshot().unwrap().unwrap();
    let marker = case.verdict_marker(node.api().as_ref(), opened.hash()).unwrap();
    let verdict = local_carrier(node, parent, peers, 0, marker, "QF1 Readout / End verdict").await;
    let outcome = case.read_verdict(node.api().as_ref(), opened.hash(), verdict.hash()).unwrap();
    let expected = if inconsistent_readout_fixture() { StopOutcome::ComputationRefuted } else { StopOutcome::ChallengeNotEstablished };
    assert_eq!(outcome, expected);
    peer_log!(
        "TB1 Court End pid={} original={} case={} verdict={} result={:?} witness={}",
        std::process::id(),
        original,
        opened.hash(),
        verdict.hash(),
        outcome,
        faster_hex::hex_string(&end_wire)
    );

    let late = carry_to_daa(node, parent, peers, input.terms().deadline_exclusive_daa, marker).await;
    let checkpoint = checkpoint_dir.join("after-terminal-deadline");
    node.api().checkpoint_database(&checkpoint);
    let recovered = reopen_checkpoint(&node.config, &checkpoint);
    let api = recovered.as_ref();
    assert_eq!(api.get_sink(), late.hash());
    let reconstructed = recover_exec(api);
    assert!(reconstructed.check_live(api).is_err());
    assert!(reconstructed.prepare_stop_case(api, original_receipt, body, owner, &witness).is_err());
    assert!(reconstructed.prepare_end_case(api, original_receipt, body, owner, &end_wire).is_err());
    let stop = reconstructed.recorded_stop_case(api, original_receipt, body, owner, &witness).unwrap();
    assert_eq!(stop.read_verdict(api, stop_case.hash(), stop_verdict.hash()).unwrap(), StopOutcome::ChallengeNotEstablished);
    let end = reconstructed.recorded_end_case(api, original_receipt, body, owner, &end_wire).unwrap();
    assert!(end.verdict_marker(api, opened.hash()).is_err());
    assert_eq!(end.read_verdict(api, opened.hash(), verdict.hash()).unwrap(), expected);
    assert_eq!(end.read_verdict(api, opened.hash(), late.hash()).unwrap_err().kind(), std::io::ErrorKind::TimedOut);
    let branch = CandidateHistory { api, candidate: None };
    let historical = recover_branch_exec(&branch, late.hash(), &api.get_native_settlement_snapshot().unwrap().unwrap());
    let read = |claim, case, verdict| historical.read_published_terminal_verdict(&branch, claim, case, verdict);
    assert_eq!(read(original, stop_case.hash(), stop_verdict.hash()).unwrap(), StopOutcome::ChallengeNotEstablished);
    assert_eq!(read(original, opened.hash(), verdict.hash()).unwrap(), expected);
    assert!(read(Hash64::default(), opened.hash(), verdict.hash()).is_err());
    assert!(read(original, stop_case.hash(), verdict.hash()).is_err());
    assert!(read(original, opened.hash(), opened.hash()).is_err());
    assert_eq!(read(original, opened.hash(), late.hash()).unwrap_err().kind(), std::io::ErrorKind::TimedOut);
    let earlier = recover_branch_exec(&branch, stop_verdict.hash(), &before_end);
    assert!(earlier.read_published_terminal_verdict(&branch, original, opened.hash(), verdict.hash()).is_err());
    assert_eq!(
        earlier.read_published_terminal_verdict(&branch, original, stop_case.hash(), stop_verdict.hash()).unwrap(),
        StopOutcome::ChallengeNotEstablished
    );
    use misaka_palw_remote::tb1_exposure::exec_input::terminal_case::PublishedTerminalCase;
    let completed = historical.read_terminal_court_at_parent(&branch, original, original_receipt, 128).unwrap();
    assert_eq!(historical.read_terminal_court_at_parent(&branch, original, republished.hash(), 128).unwrap(), completed,);
    let stop = PublishedTerminalCase {
        block: stop_case.hash(),
        receipt: original_receipt,
        relation: Qf1TerminalRelation::Stop,
        verdict: Some((stop_verdict.hash(), StopOutcome::ChallengeNotEstablished)),
    };
    let end = PublishedTerminalCase {
        block: opened.hash(),
        receipt: original_receipt,
        relation: Qf1TerminalRelation::End,
        verdict: Some((verdict.hash(), expected)),
    };
    assert_eq!(completed.cases(), &[stop, end]);
    assert_eq!(completed.blocks_success(), expected == StopOutcome::ComputationRefuted);
    let pending = recover_branch_exec(&branch, opened.hash(), &at_case)
        .read_terminal_court_at_parent(&branch, original, original_receipt, 128)
        .unwrap();
    assert!(pending.blocks_success());
    assert_eq!(pending.cases(), &[stop, PublishedTerminalCase { verdict: None, ..end }]);
    let before = earlier.read_terminal_court_at_parent(&branch, original, original_receipt, 128).unwrap();
    assert_eq!(before.cases(), &[stop]);
    let exhausted = historical.read_terminal_court_at_parent(&branch, original, original_receipt, 1).unwrap_err();
    let reason = exhausted.get_ref().and_then(|error| error.downcast_ref::<Tb1HistoryError>()).unwrap();
    assert!(matches!(reason, Tb1HistoryError::ScanBudgetExceeded { component: "qf1_terminal", limit: 1, .. }));
    assert_eq!(reason.action(), Tb1HistoryAction::InspectScanBudget);
    // Only this reopened test copy loses the case body. Missing history must
    // not turn an unresolved or refuted claim into a cleared one.
    let vp = recovered.virtual_processor();
    let mut prune = rocksdb::WriteBatch::default();
    vp.block_transactions_store.delete_batch(&mut prune, opened.hash()).unwrap();
    vp.db.write(prune).unwrap();
    assert!(historical.read_terminal_court_at_parent(&branch, original, original_receipt, 128).is_err());
    peer_log!(
        "QF1 terminal history pid={} cases={} blocks_success={}",
        std::process::id(),
        completed.cases().len(),
        completed.blocks_success()
    );
    peer_log!("QF1 public terminal verdicts pid={} parent={} original={} end={expected:?}", std::process::id(), late.hash(), original);
    peer_log!(
        "TB1 Court recovered pid={} deadline_tip={} end_verdict={} result={expected:?}",
        std::process::id(),
        late.hash(),
        verdict.hash()
    );
}
