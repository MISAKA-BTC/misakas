//! redteam — a MALICIOUS block-submission harness for a live MISAKA PALW node (the 2026-09-07
//! forged-output red-team; RFC-0002 Phase F drill D-F3 runs it with the producer on an IR class).
//!
//! It plays the adversary: a party that submits blocks WITHOUT having run the pinned inference the
//! ConsensusV2 lane grades. Each attack takes a real block template from an honest node, tampers
//! with it, submits it, and records the verdict. A correctly-behaving network rejects every one
//! (`SubmitBlockReport::Reject`); the harness exits 1 if any is accepted. Safe on a drill: a refused
//! block fails local validation and is never gossiped.
//!
//! The eight attacks: raw_skeleton, nonce_grind, algo_downgrade_khh (a genuinely solved kHeavyHash
//! nonce), forged_commitment, tamper_state_root, fat_coinbase, inflate_position,
//! insider_wellformed_envelope (a shape-valid PAV2 attempt with a fabricated output).
//!
//! Usage: redteam --rpc 127.0.0.1:37810 --network-id devnet [--attack all|dump]
//!                [--class-id <128 hex>] [--artifact-root <128 hex>] [--pay-address <address>]
//!
//! `--class-id` / `--artifact-root` name the class the insider envelope claims (D-F3: the IR class
//! the drill's producer mines, so the insider forgery is refused on the IR class's own path);
//! filler values otherwise. `--pay-address` is the templates' pay address: a testnet-12 drill serves
//! templates only to its own keyring's addresses (ADR-0152 §8.2), so D-F3 passes one; the all-zero
//! ML-DSA-87 burn address otherwise.

use kaspa_addresses::{Address, Prefix, Version};
use kaspa_consensus_core::header::Header;
use kaspa_consensus_core::network::NetworkId;
use kaspa_grpc_client::GrpcClient;
use kaspa_notify::subscription::context::SubscriptionContext;
use kaspa_rpc_core::{
    api::rpc::RpcApi, notify::mode::NotificationMode, RpcAddress, RpcRawBlock, SubmitBlockReport,
};
use std::str::FromStr;

fn parse_hash64(text: &str) -> kaspa_hashes::Hash64 {
    let t = text.trim().trim_start_matches("0x");
    assert_eq!(t.len(), 128, "a Hash64 is 128 hex characters: {text}");
    let mut out = [0u8; 64];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&t[2 * i..2 * i + 2], 16).expect("hex");
    }
    kaspa_hashes::Hash64::from_bytes(out)
}

fn burn_address(prefix: Prefix) -> Address {
    Address::new(prefix, Version::PubKeyHashMlDsa87, &[0u8; 64])
}

/// A deterministic non-crypto PRNG so runs are reproducible.
fn prng(seed: u64, n: usize) -> Vec<u8> {
    let mut x = seed ^ 0x9E37_79B9_7F4A_7C15;
    let mut out = Vec::with_capacity(n);
    while out.len() < n {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        out.extend_from_slice(&x.to_le_bytes());
    }
    out.truncate(n);
    out
}

struct Ctx {
    client: GrpcClient,
    net_id_bytes: Vec<u8>,
    pay: Address,
}

impl Ctx {
    async fn fresh_template(&self) -> RpcRawBlock {
        self.client
            .get_block_template(RpcAddress::from(self.pay.clone()), vec![])
            .await
            .expect("get_block_template")
            .block
    }

    async fn submit(&self, block: RpcRawBlock) -> Result<SubmitBlockReport, String> {
        self.client.submit_block(block, true).await.map(|r| r.report).map_err(|e| e.to_string())
    }
}

/// Print the verdict for one attack. Returns true if the network BLOCKED the attack.
fn verdict(name: &str, desc: &str, res: &Result<SubmitBlockReport, String>) -> bool {
    match res {
        Ok(SubmitBlockReport::Reject(reason)) => {
            println!("  [BLOCKED]  {name:24}  Reject({reason})   — {desc}");
            true
        }
        Ok(SubmitBlockReport::Success) => {
            println!("  [!! ACCEPTED !!] {name:24}  SUCCESS   — {desc}  <<< NETWORK ACCEPTED A FORGERY");
            false
        }
        Err(e) => {
            // A transport/RPC error that carries the validation rejection also counts as blocked;
            // the message names the rule. Anything else is inconclusive.
            let blocked = e.contains("invalid") || e.contains("reject") || e.contains("PALW") || e.contains("palw")
                || e.contains("pow") || e.contains("PoW") || e.contains("coinbase") || e.contains("algo");
            let tag = if blocked { "[BLOCKED]" } else { "[inconclusive]" };
            println!("  {tag}  {name:24}  Err: {e}   — {desc}");
            blocked
        }
    }
}

#[tokio::main]
async fn main() {
    let mut rpc = "127.0.0.1:37810".to_string();
    let mut network = "devnet".to_string();
    let mut attack = "all".to_string();
    let mut class_id: Option<kaspa_hashes::Hash64> = None;
    let mut artifact_root: Option<kaspa_hashes::Hash64> = None;
    let mut pay_address: Option<String> = None;
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--rpc" => rpc = it.next().unwrap(),
            "--network-id" => network = it.next().unwrap(),
            "--attack" => attack = it.next().unwrap(),
            "--class-id" => class_id = Some(parse_hash64(&it.next().unwrap())),
            "--artifact-root" => artifact_root = Some(parse_hash64(&it.next().unwrap())),
            "--pay-address" => pay_address = Some(it.next().unwrap()),
            other => eprintln!("ignoring arg {other}"),
        }
    }
    let net = NetworkId::from_str(&network).expect("bad network id");
    let prefix: Prefix = net.into();
    let pay = match pay_address {
        Some(text) => {
            let address = Address::try_from(text.as_str()).unwrap_or_else(|e| panic!("--pay-address {text}: {e}"));
            assert_eq!(address.prefix, prefix, "--pay-address {text} is not a {network} address");
            address
        }
        None => burn_address(prefix),
    };

    let sctx = SubscriptionContext::new();
    let client = GrpcClient::connect_with_args(
        NotificationMode::Direct,
        format!("grpc://{rpc}"),
        Some(sctx),
        true,
        None,
        false,
        Some(500_000),
        Default::default(),
    )
    .await
    .expect("connect");
    let ctx = Ctx { client, net_id_bytes: network.clone().into_bytes(), pay };
    eprintln!("[redteam] connected to {rpc} network={network}");

    // Always dump one honest template so the reader sees what an outsider actually holds.
    let t = ctx.fresh_template().await;
    let h = &t.header;
    eprintln!("=== honest algo-{} template (what an outsider gets from get_block_template) ===", h.pow_algo_id);
    eprintln!("  bits {:#010x}  nonce {}  daa {}  blue {}  state_root[..8] {}", h.bits, h.nonce, h.daa_score, h.blue_score,
              &h.palw_state_root.to_string()[..16]);
    eprintln!("  palw_commitment {} bytes   coinbase outs {}   #txs {}", h.palw_commitment.len(),
              t.transactions.first().map(|c| c.outputs.len()).unwrap_or(0), t.transactions.len());
    eprintln!();

    if attack == "dump" {
        return;
    }

    println!("=== LIVE forgery battery against the algo-6 ConsensusV2 node (outsider, no bond, no inference) ===");
    let mut blocked = 0usize;
    let mut total = 0usize;
    let mut run = |ok: bool| {
        total += 1;
        if ok {
            blocked += 1;
        }
    };

    // A1 — submit the honest skeleton UNCHANGED. It carries no attempt envelope / no coinbase
    // subsidy: a non-producer cannot turn a template into a block.
    {
        let b = ctx.fresh_template().await;
        run(verdict("raw_skeleton", "the bare template, unsolved (no inference, no attempt envelope)", &ctx.submit(b).await));
    }

    // A2 — nonce grinding. On the inference lane the tag ignores the nonce, so brute force buys
    // nothing: sweeping the nonce never turns a non-producer into a winner.
    {
        let mut b = ctx.fresh_template().await;
        b.header.nonce = 0xDEAD_BEEF_CAFE_F00D;
        run(verdict("nonce_grind", "a swept nonce with no inference behind it", &ctx.submit(b).await));
    }

    // A3 — algo downgrade. Set pow_algo_id = 1 (kHeavyHash) and ACTUALLY solve that hash target,
    // trying to sneak a genuinely PoW-solved hash block onto an inference-only network.
    // ADR-0007: two algo_id values never coexist on one network — so even valid hash-PoW is refused.
    {
        let mut b = ctx.fresh_template().await;
        b.header.pow_algo_id = 1;
        b.header.palw_state_root = kaspa_hashes::ZERO_HASH64; // algo-1 carries no PALW state root
        if let Ok(hdr) = Header::try_from(&b.header) {
            let state = kaspa_pow::StateLayer0::new(&hdr, &ctx.net_id_bytes);
            let mut n = 0u64;
            let mut solved = None;
            while n < 2_000_000 {
                if state.check_pow_layer0(n).map(|(ok, _)| ok).unwrap_or(false) {
                    solved = Some(n);
                    break;
                }
                n += 1;
            }
            match solved {
                Some(n) => {
                    b.header.nonce = n;
                    eprintln!("  (algo_downgrade: solved a real kHeavyHash nonce={n})");
                    run(verdict("algo_downgrade_khh", "a genuinely hash-PoW-solved block on the inference-only net", &ctx.submit(b).await));
                }
                None => {
                    println!("  [skip]     algo_downgrade_khh        (could not solve kHeavyHash in budget)");
                }
            }
        } else {
            println!("  [skip]     algo_downgrade_khh        (header convert failed)");
        }
    }

    // A4 — forged commitment. Fill palw_commitment with 200 random bytes, pretending to carry an
    // attempt envelope proving an inference that never ran.
    {
        let mut b = ctx.fresh_template().await;
        b.header.palw_commitment = prng(0xA4, 200);
        run(verdict("forged_commitment", "200 random bytes posing as an inference/attempt envelope", &ctx.submit(b).await));
    }

    // A5 — tampered state root. Advance/alter the committed PALW chain-state root to claim a
    // state the fold never produced.
    {
        let mut b = ctx.fresh_template().await;
        let mut bytes = b.header.palw_state_root.as_bytes();
        bytes[0] ^= 0xFF;
        b.header.palw_state_root = kaspa_hashes::Hash64::from_bytes(bytes);
        run(verdict("tamper_state_root", "a PALW state root the fold never computed", &ctx.submit(b).await));
    }

    // A6 — fat coinbase. Add an output paying the attacker a large subsidy the emission rules
    // never granted.
    {
        let mut b = ctx.fresh_template().await;
        if let Some(cb) = b.transactions.first_mut() {
            let spk = cb.outputs.first().map(|o| o.script_public_key.clone()).unwrap_or_default();
            cb.outputs.push(kaspa_rpc_core::RpcTransactionOutput {
                value: 1_000_000_000_000_000,
                script_public_key: spk,
                verbose_data: None,
            });
        }
        run(verdict("fat_coinbase", "an extra 1e15-sompi coinbase output the emission never granted", &ctx.submit(b).await));
    }

    // A7 — position inflation. Bump blue_score/daa to claim chain position/weight not earned.
    {
        let mut b = ctx.fresh_template().await;
        b.header.blue_score += 10_000;
        b.header.daa_score += 10_000;
        run(verdict("inflate_position", "a blue_score/daa jump claiming unearned chain position", &ctx.submit(b).await));
    }

    // A8 — INSIDER forgery: a STRUCTURALLY VALID attempt envelope (correct PAV2 magic, right-length
    // signature, non-zero pwu/chunks) but with a fabricated inference the attacker never ran and a
    // signature/challenge that binds nothing. It clears the shape gate the outsider forgeries died
    // on, so it must be rejected DEEPER — at the stateless challenge/signature check.
    {
        use kaspa_consensus_core::palw_attempt_v2::{
            PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, PALW_ATTEMPT_V2_VERSION,
        };
        use kaspa_consensus_core::tx::TransactionOutpoint;
        let h = |b: u8| kaspa_hashes::Hash64::from_bytes([b; 64]);
        let attempt = PalwAttemptUnsignedV2 {
            version: PALW_ATTEMPT_V2_VERSION,
            network_domain: h(0x11),
            challenge: h(0x22), // NOT the header-position challenge — a free draw
            class_id: class_id.unwrap_or(h(0x33)), // D-F3: the drill's IR class
            executor_bond: TransactionOutpoint { transaction_id: h(0x44), index: 0 },
            executor_pubkey: vec![0x55u8; 32], // not any bonded key
            operator_id: h(0x66),
            artifact_root: artifact_root.unwrap_or(h(0x77)), // D-F3: the IR class's own inventory root
            trace_root: h(0x88),
            output_root: h(0x99),    // a fabricated inference output
            pwu: 1,
            trace_manifest_root: h(0xAA),
            trace_chunk_count: 1,
            trace_retention_daa: 1,
            execution_root: h(0xBB),
        };
        let env = PalwAttemptEnvelopeV2 { attempt, signature: vec![0u8; 4627] };
        let mut b = ctx.fresh_template().await;
        b.header.palw_commitment = env.encode_wire();
        run(verdict(
            "insider_wellformed_envelope",
            "a shape-valid PAV2 attempt with a fabricated output and a signature/challenge that binds nothing",
            &ctx.submit(b).await,
        ));
    }

    println!();
    println!("=== result: {blocked}/{total} forgeries BLOCKED by the live node ===");
    if blocked == total {
        println!("every outsider forgery was rejected — a party that did not run the inference cannot land a block.");
    } else {
        println!("!!! {} forgery/forgeries were ACCEPTED — investigate above.", total - blocked);
        std::process::exit(1);
    }
}
