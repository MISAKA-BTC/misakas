//! **`misaka-palw-pool-miner` — mine the PALW floor class with no node, no chain and no model
//! file.**
//!
//! ```text
//! misaka-palw-pool-miner --pool HOST:PORT \
//!     --bond <txid>:<index> --key /path/to/seed.hex --pay-address misakatest:...
//! ```
//!
//! # What this needs, and what it deliberately does not
//!
//! It needs three things: somewhere to connect, a **bond the chain already holds**, and the
//! ML-DSA-87 seed whose verification key that bond registered. It does not need a synced node, an
//! open port, tens of gigabytes of chain state, or a downloaded model — the floor class's weights
//! are derived from a pinned seed, so the artifact is minted in memory at startup and checked
//! against the root the chain registered.
//!
//! # The key never leaves this process
//!
//! The seed is read with the same hardened loader the validator uses (owner-only permissions, no
//! symlinks, fail closed) and is used for exactly two signatures: one over the pool's session
//! challenge, under the POOL AUTH context, and one per won block, under the ATTEMPT context.
//! Those contexts are different, which is what makes it impossible for a pool to obtain the
//! second by asking for the first.
//!
//! # Registering the bond this wants
//!
//! `kaspad --palw-register-bond` on any node (yours or a friend's) — it is a one-off transaction,
//! and afterwards the bond is chain state that any pool can look up. The pool refuses a miner
//! whose bond it cannot find, and says so.

use kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2;
use kaspa_hashes::Hash64;
use misaka_palw_pool::miner::{MinerConfigV1, WelcomeV1, decode_job_v1, resolve_class_v1, work_one_job_v1};
use misaka_palw_pool::protocol::{MinerMessageV1, PoolMessageV1, SolutionV1, encode_line, from_hex, parse_line, to_hex};
use misaka_palw_pool::session::sign_pool_auth_v1;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

fn die(message: String) -> ! {
    eprintln!("misaka-palw-pool-miner: {message}");
    std::process::exit(1)
}

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(|s| s.as_str())
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let usage = "usage: misaka-palw-pool-miner --pool HOST:PORT --bond <txid>:<index> --key SEED_HEX_FILE --pay-address ADDR";
    let pool = flag(&args, "--pool").unwrap_or_else(|| die(usage.into())).to_string();
    let bond_text = flag(&args, "--bond").unwrap_or_else(|| die(usage.into())).to_string();
    let key_path = flag(&args, "--key").unwrap_or_else(|| die(usage.into())).to_string();
    let pay_address = flag(&args, "--pay-address").unwrap_or_else(|| die(usage.into())).to_string();
    let agent = flag(&args, "--agent").unwrap_or("misaka-palw-pool-miner/1").to_string();

    let seed = kaspa_pq_validator_core::load_validator_seed(&key_path).unwrap_or_else(|e| die(e));
    let keypair = libcrux_ml_dsa::ml_dsa_87::generate_key_pair(seed);
    let pubkey = keypair.verification_key.as_ref().to_vec();
    let address: kaspa_addresses::Address = pay_address.as_str().try_into().unwrap_or_else(|e| die(format!("--pay-address: {e:?}")));
    let my_script = kaspa_txscript::pay_to_address_script(&address);
    let bond = {
        let (txid, index) = bond_text.split_once(':').unwrap_or_else(|| die(format!("'{bond_text}' is not <txid>:<index>")));
        let transaction_id: kaspa_consensus_core::tx::TransactionId =
            txid.parse().unwrap_or_else(|e| die(format!("'{txid}' is not a transaction id: {e}")));
        let index: u32 = index.parse().unwrap_or_else(|e| die(format!("'{index}' is not an output index: {e}")));
        kaspa_consensus_core::tx::TransactionOutpoint::new(transaction_id, index)
    };

    println!("misaka-palw-pool-miner");
    println!("  pool        {pool}");
    println!("  bond        {bond_text}");
    println!("  pays        {pay_address}");
    println!("  key         {key_path} (the seed never leaves this process)");

    let config = MinerConfigV1 { pool, bond_text, pay_address, agent, reconnect_after: std::time::Duration::from_secs(5) };
    // **A refusal and a dropped socket are not the same wait.** A pool that says "this chain holds
    // no bond at …" will say it again in five seconds and in five minutes — the fix is a
    // registration, not a retry — so hammering it only fills the pool operator's disk with one
    // repeated line. A socket that died is the opposite: usually transient, and worth retrying
    // promptly. `palw_producer.rs` records the same lesson from a live node ("this loop wrote
    // 5,281 identical warnings"), which is where this rule comes from.
    let refused_backoff = std::time::Duration::from_secs(60);
    let mut last_refusal: Option<String> = None;
    loop {
        let wait = match one_connection(&config, &keypair, &pubkey, &my_script, bond).await {
            Ok(()) => config.reconnect_after,
            Err(MinerExit::Transport(e)) => {
                eprintln!("[miner] {e}");
                last_refusal = None;
                config.reconnect_after
            }
            Err(MinerExit::Refused(reason)) => {
                // Printed once per DISTINCT reason. A miner waiting for its bond to confirm sees
                // the sentence it needs and then silence, rather than the same line every 5 s.
                if last_refusal.as_deref() != Some(reason.as_str()) {
                    eprintln!("[miner] the pool refused this miner: {reason}");
                    eprintln!("[miner] this will not fix itself by retrying — retrying every {refused_backoff:?} in case it is fixed");
                    last_refusal = Some(reason);
                }
                refused_backoff
            }
        };
        tokio::time::sleep(wait).await;
    }
}

/// Why a connection ended, and therefore how long to wait before the next one.
enum MinerExit {
    /// The pool refused this miner and said why. Retrying changes nothing until an operator does.
    Refused(String),
    /// The socket, the protocol, or the local state. Usually transient.
    Transport(String),
}

impl From<String> for MinerExit {
    fn from(e: String) -> Self {
        MinerExit::Transport(e)
    }
}

/// One connection: hello, prove the bond, resolve the class, then work jobs until it drops.
async fn one_connection(
    config: &MinerConfigV1,
    keypair: &libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair,
    pubkey: &[u8],
    my_script: &kaspa_consensus_core::tx::ScriptPublicKey,
    bond: kaspa_consensus_core::tx::TransactionOutpoint,
) -> Result<(), MinerExit> {
    let stream = tokio::net::TcpStream::connect(&config.pool).await.map_err(|e| format!("connecting to {}: {e}", config.pool))?;
    let (read_half, mut out) = stream.into_split();
    let mut lines = BufReader::new(read_half).lines();
    let send = async |message: MinerMessageV1, out: &mut tokio::net::tcp::OwnedWriteHalf| -> Result<(), String> {
        out.write_all(encode_line(&message)?.as_bytes()).await.map_err(|e| format!("write: {e}"))
    };

    send(
        MinerMessageV1::Hello {
            protocol: misaka_palw_pool::protocol::PALW_POOL_PROTOCOL_VERSION,
            bond: config.bond_text.clone(),
            pubkey: to_hex(pubkey),
            pay_address: config.pay_address.clone(),
            agent: config.agent.clone(),
        },
        &mut out,
    )
    .await?;

    let mut network_id = String::new();
    let mut backend: Option<Box<dyn kaspa_consensus_core::palw_backend::PalwExecutionBackendV1>> = None;
    let mut network_domain = Hash64::default();

    loop {
        let Some(line) = lines.next_line().await.map_err(|e| format!("read: {e}"))? else {
            return Err(MinerExit::Transport("the pool closed the connection".into()));
        };
        if line.trim().is_empty() {
            continue;
        }
        match parse_line::<PoolMessageV1>(line.trim())? {
            PoolMessageV1::Rejected { reason } => return Err(MinerExit::Refused(reason)),
            PoolMessageV1::Standby { reason, retry_after_ms } => {
                eprintln!("[miner] the pool has no work right now ({reason}); asking again in {retry_after_ms} ms");
                tokio::time::sleep(std::time::Duration::from_millis(retry_after_ms)).await;
                send(MinerMessageV1::JobRequest { finished: None }, &mut out).await?;
            }
            PoolMessageV1::Challenge { session_nonce, network_id: net, .. } => {
                let nonce: [u8; 32] =
                    from_hex(&session_nonce)?.try_into().map_err(|_| "the pool's session nonce is not 32 bytes".to_string())?;
                network_id = net;
                network_domain = palw_network_domain_v2(network_id.as_bytes());
                let signature =
                    sign_pool_auth_v1(&keypair.signing_key, &nonce, &network_id, &config.bond_text, pubkey, &config.pay_address)?;
                send(MinerMessageV1::Auth { signature: to_hex(&signature) }, &mut out).await?;
            }
            PoolMessageV1::Welcome { class_id, artifact_root, court, is_base_class } => {
                let welcome = WelcomeV1 {
                    class_id: class_id.parse().map_err(|e| format!("class_id: {e}"))?,
                    artifact_root: artifact_root.parse().map_err(|e| format!("artifact_root: {e}"))?,
                    court: borsh::from_slice(&from_hex(&court)?).map_err(|e| format!("court params: {e}"))?,
                    is_base_class,
                };
                // Derived, not downloaded — and refused if this machine's derivation does not hash
                // to the root the chain registered.
                let resolved = resolve_class_v1(&welcome)?;
                println!(
                    "[miner] admitted on {network_id}; class {} ({})",
                    welcome.class_id,
                    if is_base_class { "the derived floor — nothing to download" } else { "a converted class" }
                );
                backend = Some(Box::new(resolved));
                send(MinerMessageV1::JobRequest { finished: None }, &mut out).await?;
            }
            PoolMessageV1::Job(job) => {
                let Some(backend) = backend.as_deref() else {
                    return Err(MinerExit::Transport("the pool sent a job before a welcome".into()));
                };
                let decoded = decode_job_v1(&job)?;
                let job_id = decoded.job_id.clone();
                let started = std::time::Instant::now();
                // The work is pure CPU with no await in it, so it goes to a blocking thread rather
                // than pinning a tokio worker for the whole grind.
                let (net_id, domain) = (network_id.clone(), network_domain);
                let outcome = work_one_job_v1(&decoded, backend, my_script, domain, &net_id, bond, keypair, &|| false);
                match outcome {
                    Err(e) => {
                        eprintln!("[miner] job {job_id} refused: {e}");
                        send(MinerMessageV1::JobRequest { finished: Some(job_id) }, &mut out).await?;
                    }
                    Ok(None) => {
                        println!("[miner] job {job_id}: no winner in the assigned range ({:?})", started.elapsed());
                        send(MinerMessageV1::Exhausted { job_id, nonces_tried: decoded.nonce_end - decoded.nonce_start }, &mut out)
                            .await?;
                    }
                    Ok(Some((won, material))) => {
                        println!(
                            "[miner] job {job_id}: WON at nonce {} after {} tries ({:?})",
                            won.nonce,
                            won.nonces_tried,
                            started.elapsed()
                        );
                        send(
                            MinerMessageV1::Solution(Box::new(SolutionV1 {
                                job_id,
                                nonce: won.nonce,
                                envelope: to_hex(&won.envelope),
                                material: to_hex(&material),
                            })),
                            &mut out,
                        )
                        .await?;
                    }
                }
            }
            PoolMessageV1::SolutionResult { job_id, accepted, block_hash, reason } => {
                if accepted {
                    println!("[miner] block {block_hash} accepted — the coinbase pays this miner's address");
                } else {
                    eprintln!("[miner] job {job_id} was not accepted: {reason}");
                }
                send(MinerMessageV1::JobRequest { finished: None }, &mut out).await?;
            }
        }
    }
}
