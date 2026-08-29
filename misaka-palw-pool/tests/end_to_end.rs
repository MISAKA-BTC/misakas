//! **The pool, end to end: a miner with no chain produces a block over a real socket.**
//!
//! What is real here is everything except the chain: the TCP transport, the line protocol, the
//! ML-DSA-87 bond handshake, the floor class derived from its pinned seed, the anchored inference
//! on the real `Base0Backend`, the nonce grind, the attempt signature, and the pool's mounting of
//! the result into the header it handed out. What stands in is `FakeChain` — a `PoolChainV1` that
//! answers the four chain questions from a fixture instead of from consensus.
//!
//! That is the seam the pool was written around, and the substitution is deliberate: a test that
//! needed a synced network to run would not run, and the half this crate owns is exactly the half
//! that does not need one. The node-side adapter (`kaspad/src/palw_pool.rs`) is the other side of
//! the same trait, and consensus's own acceptance of a pool-shaped block is proved where the
//! consensus fixtures live.
//!
//! The class target is set wide open so the grind terminates in a test's worth of time; the
//! Layer-0 check the miner runs is the real one, so a solution that arrives has really passed it.

use kaspa_consensus_core::tx::TransactionOutpoint;
use kaspa_hashes::Hash64;
use misaka_palw_pool::protocol::{MinerMessageV1, PoolMessageV1, SolutionV1, encode_line, from_hex, parse_line, to_hex};
use misaka_palw_pool::server::{PoolChainV1, PoolStateV1, PreparedJobV1};
use misaka_palw_pool::session::{BondStandingV1, sign_pool_auth_v1};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

const NETWORK: &str = "misaka-testnet-11";

/// The chain, as far as the pool can tell. Every answer is a fixture, and `published` records what
/// the pool would have put on a real one.
struct FakeChain {
    class_id: Hash64,
    artifact_root: Hash64,
    court: kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2,
    registered_pubkey: Vec<u8>,
    /// Bonds this "chain" holds. A miner naming anything else is the unbonded case.
    known_bond: TransactionOutpoint,
    published: Mutex<Vec<(Hash64, usize, Hash64)>>,
}

#[async_trait::async_trait]
impl PoolChainV1 for FakeChain {
    fn network_id(&self) -> String {
        NETWORK.to_string()
    }
    fn address_prefix(&self) -> kaspa_addresses::Prefix {
        kaspa_addresses::Prefix::Testnet
    }
    fn job_for_class_id(&self) -> Hash64 {
        self.class_id
    }
    async fn class_facts(&self) -> (Hash64, Hash64, Vec<u8>, bool) {
        (self.class_id, self.artifact_root, borsh::to_vec(&self.court).expect("court encodes"), true)
    }
    async fn bond_standing(&self, _class_id: Hash64, bond: TransactionOutpoint) -> Result<BondStandingV1, String> {
        if bond != self.known_bond {
            return Ok(BondStandingV1 { known: false, registered_pubkey: Vec::new(), not_ready_reason: String::new() });
        }
        Ok(BondStandingV1 { known: true, registered_pubkey: self.registered_pubkey.clone(), not_ready_reason: String::new() })
    }
    async fn job_for(&self, identity: misaka_palw_pool::session::MinerIdentityV1) -> Result<PreparedJobV1, String> {
        // A template paying THIS miner, exactly as the node-side adapter builds one.
        let script = kaspa_txscript::pay_to_address_script(&identity.pay_address);
        let transactions = vec![kaspa_consensus_core::tx::Transaction::new(
            0,
            Vec::new(),
            vec![kaspa_consensus_core::tx::TransactionOutput::new(50_000_000, script)],
            0,
            kaspa_consensus_core::subnets::SUBNETWORK_ID_COINBASE,
            0,
            Vec::new(),
        )];
        let mut header = kaspa_consensus_core::header::Header::from_precomputed_hash(Hash64::from_u64_word(0xFEED), Vec::new());
        header.pow_algo_id = kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_COMMITTED_V2;
        header.hash_merkle_root = kaspa_consensus_core::merkle::calc_hash_merkle_root(transactions.iter());
        header.timestamp = 1_800_000_000_000;
        // Layer-0 has to be reachable in a test, so the header's own difficulty target is the
        // easiest the encoding allows. The CHECK the miner runs is the real one.
        header.bits = 0x207fffff;
        Ok(PreparedJobV1 {
            header,
            transactions,
            class_id: self.class_id,
            artifact_root: self.artifact_root,
            // Wide open: this test is about the path, and a real target would make it a benchmark.
            class_target: u128::MAX,
            pwu: 7900,
            operator_id: Hash64::from_u64_word(0x0B),
            trace_retention_daa: 4242,
        })
    }
    async fn publish(
        &self,
        attempt_id: Hash64,
        material: Vec<u8>,
        block: kaspa_consensus_core::block::Block,
    ) -> Result<Hash64, String> {
        // The obligation the pool takes on for a miner with no mouth: the material is recorded
        // here in the order the real adapter writes and gossips it, before the block is published.
        self.published.lock().expect("lock").push((attempt_id, material.len(), block.hash()));
        Ok(block.hash())
    }
}

fn testnet_address(byte: u8) -> String {
    kaspa_addresses::Address::new(kaspa_addresses::Prefix::Testnet, kaspa_addresses::Version::PubKeyHashMlDsa87, &[byte; 64])
        .to_string()
}

/// Stand a pool up on an ephemeral port and hand back its address and the chain behind it.
async fn pool_on_a_port(chain: Arc<FakeChain>) -> (std::net::SocketAddr, tokio::sync::watch::Sender<bool>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    let (shutdown, rx) = tokio::sync::watch::channel(false);
    let state = Arc::new(tokio::sync::Mutex::new(PoolStateV1::new()));
    tokio::spawn(async move {
        misaka_palw_pool::server::serve_v1(chain as Arc<dyn PoolChainV1>, listener, state, 8, rx).await;
    });
    (addr, shutdown)
}

/// The floor's own class id, artifact root and court — the class a pool miner needs no download
/// for, because it derives from a pinned seed.
fn floor_facts() -> (Hash64, Hash64, kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2) {
    // The shipped court: it decides the geometry a class is admissible at, and therefore its id.
    let court =
        kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2::new(kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES, 4, 2)
            .expect("the shipped court");
    let artifact_root = misaka_palw_base0::rc::palw_rc_base0_artifact_root_v1().expect("the floor's artifact derives");
    let class_id =
        kaspa_consensus_core::palw_base0_profile::base0_profile_v1(kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_GEOMETRY)
            .expect("the floor's profile projects")
            .shape_profile_id();
    (class_id, artifact_root, court)
}

/// **The whole path**: a miner with no chain says hello, proves its bond, derives the floor class
/// from nothing, runs a real anchored inference, wins, and the pool mounts and publishes it.
#[tokio::test]
async fn a_nodeless_miner_with_a_bond_produces_a_block_through_the_pool() {
    let keypair = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([0x77u8; 32]);
    let (class_id, artifact_root, court) = floor_facts();
    let bond = TransactionOutpoint::new(kaspa_consensus_core::tx::TransactionId::from_u64_word(0xB0), 0);
    let chain = Arc::new(FakeChain {
        class_id,
        artifact_root,
        court,
        registered_pubkey: keypair.verification_key.as_ref().to_vec(),
        known_bond: bond,
        published: Mutex::new(Vec::new()),
    });
    let (addr, _shutdown) = pool_on_a_port(chain.clone()).await;

    let bond_text = format!("{}:0", kaspa_consensus_core::tx::TransactionId::from_u64_word(0xB0));
    let pay_address = testnet_address(0x33);
    let my_script = kaspa_txscript::pay_to_address_script(&kaspa_addresses::Address::try_from(pay_address.as_str()).expect("address"));

    let stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
    let (read_half, mut out) = stream.into_split();
    let mut lines = BufReader::new(read_half).lines();
    let pubkey = keypair.verification_key.as_ref().to_vec();

    // Hello.
    out.write_all(
        encode_line(&MinerMessageV1::Hello {
            protocol: misaka_palw_pool::protocol::PALW_POOL_PROTOCOL_VERSION,
            bond: bond_text.clone(),
            pubkey: to_hex(&pubkey),
            pay_address: pay_address.clone(),
            agent: "e2e".into(),
        })
        .expect("encode")
        .as_bytes(),
    )
    .await
    .expect("write");

    // Challenge → the bond proof.
    let line = lines.next_line().await.expect("read").expect("a challenge");
    let PoolMessageV1::Challenge { session_nonce, network_id, .. } = parse_line(&line).expect("parse") else {
        panic!("expected a challenge, got {line}");
    };
    assert_eq!(network_id, NETWORK);
    let nonce: [u8; 32] = from_hex(&session_nonce).expect("hex").try_into().expect("32 bytes");
    let signature = sign_pool_auth_v1(&keypair.signing_key, &nonce, &network_id, &bond_text, &pubkey, &pay_address).expect("sign");
    out.write_all(encode_line(&MinerMessageV1::Auth { signature: to_hex(&signature) }).expect("encode").as_bytes())
        .await
        .expect("write");

    // Welcome → resolve the class locally, from nothing.
    let line = lines.next_line().await.expect("read").expect("a welcome");
    let PoolMessageV1::Welcome { class_id: welcomed, artifact_root: root, court: court_hex, is_base_class } =
        parse_line(&line).expect("parse")
    else {
        panic!("expected a welcome, got {line}");
    };
    assert!(is_base_class, "the pool serves the derived floor");
    let welcome = misaka_palw_pool::miner::WelcomeV1 {
        class_id: welcomed.parse().expect("class id"),
        artifact_root: root.parse().expect("artifact root"),
        court: borsh::from_slice(&from_hex(&court_hex).expect("hex")).expect("court"),
        is_base_class,
    };
    assert_eq!(welcome.class_id, class_id);
    // **Nothing is downloaded.** The floor's weights are derived here and refused unless they hash
    // to the root the chain registered.
    let backend = misaka_palw_pool::miner::resolve_class_v1(&welcome).expect("the floor resolves from nothing");

    // Ask for work.
    out.write_all(encode_line(&MinerMessageV1::JobRequest { finished: None }).expect("encode").as_bytes()).await.expect("write");
    let line = lines.next_line().await.expect("read").expect("a job");
    let PoolMessageV1::Job(job) = parse_line(&line).expect("parse") else { panic!("expected a job, got {line}") };
    let decoded = misaka_palw_pool::miner::decode_job_v1(&job).expect("the job decodes");
    assert_eq!(decoded.pwu, 7900);
    assert_eq!(decoded.trace_retention_daa, 4242);

    // The job really pays this miner, and the header really commits to the coinbase that says so.
    misaka_palw_pool::miner::verify_job_pays_me(&decoded, &my_script).expect("the pool built a template that pays this miner");

    // The work: a real anchored inference on the floor, then the real grind and signature.
    let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2(NETWORK.as_bytes());
    let (won, material) = tokio::task::spawn_blocking({
        let decoded_script = my_script.clone();
        move || {
            misaka_palw_pool::miner::work_one_job_v1(&decoded, &backend, &decoded_script, domain, NETWORK, bond, &keypair, &|| false)
        }
    })
    .await
    .expect("the work task finishes")
    .expect("the work runs")
    .expect("a wide-open class target and the easiest Layer-0 target win inside one range");
    assert!(!material.is_empty(), "a real execution produces material for the pool to serve");

    // Ship it.
    out.write_all(
        encode_line(&MinerMessageV1::Solution(Box::new(SolutionV1 {
            job_id: job.job_id.clone(),
            nonce: won.nonce,
            envelope: to_hex(&won.envelope),
            material: to_hex(&material),
        })))
        .expect("encode")
        .as_bytes(),
    )
    .await
    .expect("write");

    let line = lines.next_line().await.expect("read").expect("a result");
    let PoolMessageV1::SolutionResult { accepted, block_hash, reason, .. } = parse_line(&line).expect("parse") else {
        panic!("expected a solution result, got {line}");
    };
    assert!(accepted, "the pool refused a solution it handed the job for: {reason}");
    assert!(!block_hash.is_empty());

    // And the pool discharged the obligation the miner cannot: the material was published with the
    // block, under the attempt id a challenge would name.
    let published = chain.published.lock().expect("lock");
    assert_eq!(published.len(), 1, "one block, one material publication");
    assert_eq!(published[0].1, material.len(), "the pool served the miner's own bytes, whole");
    assert_eq!(published[0].2.to_string(), block_hash);
}

/// **A miner the chain holds no bond for is turned away at the door**, before a challenge, with a
/// refusal that says what to do about it.
#[tokio::test]
async fn a_miner_without_a_bond_is_refused_before_it_is_asked_to_sign() {
    let keypair = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([0x77u8; 32]);
    let (class_id, artifact_root, court) = floor_facts();
    let chain = Arc::new(FakeChain {
        class_id,
        artifact_root,
        court,
        registered_pubkey: keypair.verification_key.as_ref().to_vec(),
        // The chain holds THIS bond, and the miner below will name a different one.
        known_bond: TransactionOutpoint::new(kaspa_consensus_core::tx::TransactionId::from_u64_word(0xB0), 0),
        published: Mutex::new(Vec::new()),
    });
    let (addr, _shutdown) = pool_on_a_port(chain).await;

    let stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
    let (read_half, mut out) = stream.into_split();
    let mut lines = BufReader::new(read_half).lines();
    out.write_all(
        encode_line(&MinerMessageV1::Hello {
            protocol: misaka_palw_pool::protocol::PALW_POOL_PROTOCOL_VERSION,
            bond: format!("{}:0", kaspa_consensus_core::tx::TransactionId::from_u64_word(0xDEAD)),
            pubkey: to_hex(keypair.verification_key.as_ref()),
            pay_address: testnet_address(0x33),
            agent: "unbonded".into(),
        })
        .expect("encode")
        .as_bytes(),
    )
    .await
    .expect("write");

    let line = lines.next_line().await.expect("read").expect("a rejection");
    let PoolMessageV1::Rejected { reason } = parse_line(&line).expect("parse") else {
        panic!("an unbonded miner must be rejected, got {line}");
    };
    assert!(reason.contains("holds no bond"), "the refusal names the missing bond: {reason}");
    assert!(reason.contains("--palw-register-bond"), "and says how to get one: {reason}");
    // The socket is closed rather than left open for a miner that can never be served.
    assert!(lines.next_line().await.expect("read").is_none(), "a refused miner is hung up on");
}

/// **A miner that is bonded but does not hold the key gets no work.** The pool believes the chain
/// about which key a bond is, and a signature is the only way to claim it.
#[tokio::test]
async fn a_bonded_miner_that_cannot_sign_is_refused_at_auth() {
    let real = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([0x77u8; 32]);
    let impostor = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([0x88u8; 32]);
    let (class_id, artifact_root, court) = floor_facts();
    let bond = TransactionOutpoint::new(kaspa_consensus_core::tx::TransactionId::from_u64_word(0xB0), 0);
    let chain = Arc::new(FakeChain {
        class_id,
        artifact_root,
        court,
        registered_pubkey: real.verification_key.as_ref().to_vec(),
        known_bond: bond,
        published: Mutex::new(Vec::new()),
    });
    let (addr, _shutdown) = pool_on_a_port(chain).await;

    let bond_text = format!("{}:0", kaspa_consensus_core::tx::TransactionId::from_u64_word(0xB0));
    let pay_address = testnet_address(0x33);
    let stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
    let (read_half, mut out) = stream.into_split();
    let mut lines = BufReader::new(read_half).lines();

    // Claims the real bond AND the real public key — but signs with the key it actually has.
    out.write_all(
        encode_line(&MinerMessageV1::Hello {
            protocol: misaka_palw_pool::protocol::PALW_POOL_PROTOCOL_VERSION,
            bond: bond_text.clone(),
            pubkey: to_hex(real.verification_key.as_ref()),
            pay_address: pay_address.clone(),
            agent: "impostor".into(),
        })
        .expect("encode")
        .as_bytes(),
    )
    .await
    .expect("write");

    let line = lines.next_line().await.expect("read").expect("a challenge");
    let PoolMessageV1::Challenge { session_nonce, network_id, .. } = parse_line(&line).expect("parse") else {
        panic!("expected a challenge, got {line}");
    };
    let nonce: [u8; 32] = from_hex(&session_nonce).expect("hex").try_into().expect("32 bytes");
    let signature =
        sign_pool_auth_v1(&impostor.signing_key, &nonce, &network_id, &bond_text, real.verification_key.as_ref(), &pay_address)
            .expect("sign");
    out.write_all(encode_line(&MinerMessageV1::Auth { signature: to_hex(&signature) }).expect("encode").as_bytes())
        .await
        .expect("write");

    let line = lines.next_line().await.expect("read").expect("a rejection");
    let PoolMessageV1::Rejected { reason } = parse_line(&line).expect("parse") else {
        panic!("an impostor must be rejected, got {line}");
    };
    assert!(reason.contains("did not verify"), "the refusal is about the signature: {reason}");
}
