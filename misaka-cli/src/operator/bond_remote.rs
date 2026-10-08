//! **`misaka bond register-export | register-sign | register-submit`** — a bond registered without a node of one's own (RFC-0009 stage A0,
//! H1's `NODELESS_BOND_REGISTRATION_ABSENT`). The library half is `misaka_palw_remote::bondreg`; this file reads the nodes, prices the
//! carrier the way a node relays it, and talks to the user.
//!
//! * `register-export` holds NO key: the bond's public key, the collateral, the payout, the payer's address → an unsigned bundle. The
//!   floor is this build's own ruleset (`palw_bond_registration_floor_v1`); the funding UTXO and the DAA are the nodes' word (≥ 2 nodes
//!   unless `--allow-single-rpc`), every node's ruleset must be this build's.
//! * `register-sign` is offline: it re-derives what it signs, refuses a bundle that moves the payout, the collateral, the change or the
//!   fee, and — below VERIFIED_REMOTE, which an exported bundle always is — signs only on `--accept-unverified-state`.
//! * `register-submit` holds no key: it verifies the signed bytes, relays them through any nodes, and follows the bond (`<carrier>:0`)
//!   until a node reports it registered (an ACK is not inclusion).

use std::path::PathBuf;
use std::time::Duration;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_rpc_core::api::rpc::RpcApi;
use misaka_palw_remote::bondreg::{
    BondBundleV1, BondSignerPolicyV1, SignedBondRegistrationV1, bond_carrier_body_v1, bond_carrier_sign_v1, bond_object_of_v1,
    bond_owner_sign_v1, build_bond_bundle_v1, own_payout_payload_v1, verify_signed_bond_v1,
};
use misaka_palw_remote::bundle::{FundingInputV1, placeholder_funding_script_v1};
use misaka_palw_remote::verify::{ModeLabelV1, signing_gate_v1};

use crate::operator::model_bundle::KeySigner;
use crate::operator::snapshot;
use crate::{CliError, CliResult, exit};

/// How long an export's quote stands, in DAA (as the class registration's).
const BOND_QUOTE_VALIDITY_DAA: u64 = 120;

pub(crate) struct ExportArgs {
    pub(crate) owner_pubkey: String,
    pub(crate) collateral: u64,
    pub(crate) payout_address: Option<String>,
    pub(crate) payer_address: String,
    pub(crate) quote_rpc: Vec<String>,
    pub(crate) allow_single_rpc: bool,
    pub(crate) out: PathBuf,
}

pub(crate) struct SignArgs {
    pub(crate) bundle: PathBuf,
    pub(crate) key: crate::keys::KeySource,
    pub(crate) payer_key_file: Option<String>,
    pub(crate) expect_collateral: u64,
    pub(crate) expect_payout: Option<String>,
    pub(crate) max_fee_sompi: u64,
    pub(crate) accept_unverified_state: Option<String>,
    pub(crate) out: Option<PathBuf>,
}

pub(crate) struct SubmitArgs {
    pub(crate) signed: PathBuf,
    pub(crate) relay: Vec<String>,
    pub(crate) relay_min: Option<usize>,
    pub(crate) max_fee_sompi: Option<u64>,
    pub(crate) no_wait: bool,
}

fn err(code: i32, msg: impl Into<String>) -> CliError {
    CliError::new(code, msg.into())
}

struct Chain {
    params: kaspa_consensus_core::config::params::Params,
    domain: Hash64,
    ruleset: String,
    schedule: String,
    floor: u64,
    base_class: Hash64,
}

fn chain_of(ctx: &crate::node::Ctx) -> Result<Chain, CliError> {
    let net: kaspa_consensus_core::network::NetworkId =
        ctx.network.parse().map_err(|e| err(exit::CONFIG, format!("'{}' is not a network id: {e}", ctx.network)))?;
    let (params, _) = crate::wallet::chain_params(ctx, net)?;
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
        return Err(err(exit::CONFIG, format!("{net} has no PALW bonds")));
    };
    let floor = kaspa_consensus_core::palw_state_v2::palw_bond_registration_floor_v1(
        bundle.state.min_collateral_sompi(),
        params.palw_audit_2026_09_23_active_at(0),
    );
    let base_class = bundle.base_class_id;
    Ok(Chain {
        domain: kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
            params.net.to_string().as_bytes(),
            Some(params.genesis.hash),
        ),
        ruleset: params.consensus_params_id().to_string(),
        schedule: params.consensus_schedule_id().to_string(),
        floor,
        base_class,
        params,
    })
}

fn hex_bytes(text: &str, what: &str) -> Result<Vec<u8>, CliError> {
    let mut out = vec![0u8; text.len() / 2];
    if text.len() % 2 != 0 || faster_hex::hex_decode(text.as_bytes(), &mut out).is_err() {
        return Err(err(exit::GENERIC, format!("{what} is not hex")));
    }
    Ok(out)
}

fn payload_of(address: &str, prefix: kaspa_addresses::Prefix, what: &str) -> Result<Hash64, CliError> {
    let a = kaspa_addresses::Address::try_from(address).map_err(|e| err(exit::GENERIC, format!("{what}: {e}")))?;
    if a.prefix != prefix || a.version != kaspa_addresses::Version::PubKeyHashMlDsa87 {
        return Err(err(exit::GENERIC, format!("{what} is not an ML-DSA-87 address of this network")));
    }
    let bytes: [u8; 64] =
        a.payload.as_slice().try_into().map_err(|_| err(exit::GENERIC, format!("{what}: a 64-byte payload is expected")))?;
    Ok(Hash64::from_bytes(bytes))
}

/// The carrier's fee as a node relays it, priced on the body with a signature script of the real length; refused when its storage
/// mass is past the relay limit (the collateral output is large and the change small — `kaspad --palw-register-bond`'s own refusal).
fn price(chain: &Chain, b: &BondBundleV1) -> Result<u64, CliError> {
    use kaspa_consensus_core::mass::MassCalculator;
    let calc = MassCalculator::new(
        chain.params.mass_per_tx_byte,
        chain.params.mass_per_script_pub_key_byte,
        chain.params.mass_per_sig_op,
        chain.params.storage_mass_parameter,
    );
    let object = bond_object_of_v1(b).map_err(|e| err(exit::GENERIC, e.to_string()))?;
    let mut probe_b = b.clone();
    // a signature of the real length in the payload (two halves past the possession fence)
    probe_b.signature =
        faster_hex::hex_string(&vec![0u8; kaspa_txscript::MLDSA87_SIG_LEN * if b.operator_possession { 2 } else { 1 }]);
    let probe_obj = bond_object_of_v1(&probe_b).map_err(|e| err(exit::GENERIC, e.to_string()))?;
    let _ = object;
    let probe =
        bond_carrier_body_v1(&probe_b, &probe_obj, placeholder_funding_script_v1()).map_err(|e| err(exit::FUNDS, e.to_string()))?;
    let compute = calc.calc_non_contextual_masses(&probe).compute_mass;
    let floor = kaspa_pq_validator_core::ATTESTATION_TX_FEE_FLOOR_SOMPI;
    let fee = kaspa_pq_validator_core::relay_fee_for_compute_mass(compute).max(floor);
    let entry = kaspa_consensus_core::tx::UtxoEntry::new(
        b.funding.amount,
        b.payer_spk.parse().map_err(|_| err(exit::GENERIC, "the payer script"))?,
        b.funding.block_daa_score,
        b.funding.is_coinbase,
    );
    let storage = calc
        .calc_contextual_masses(&kaspa_consensus_core::tx::PopulatedTransaction::new(&probe, vec![entry]))
        .map(|m| m.storage_mass)
        .unwrap_or(u64::MAX);
    if storage > kaspa_consensus_core::palw_mode_v2::PALW_MIRRORED_STANDARD_TX_MASS {
        return Err(err(
            exit::FUNDS,
            format!(
                "this funding output cannot carry a {} sompi collateral within the relay mass limit (storage mass {storage}): fund the \
                 payer with more, or change the collateral",
                b.collateral
            ),
        ));
    }
    Ok(fee)
}

/// `misaka bond register-export`.
pub(crate) async fn export(ctx: &crate::node::Ctx, args: ExportArgs) -> CliResult {
    let chain = chain_of(ctx)?;
    let prefix = chain.params.prefix();
    let pubkey = hex_bytes(&args.owner_pubkey, "--owner-pubkey")?;
    if pubkey.len() != kaspa_txscript::MLDSA87_PK_LEN {
        return Err(err(exit::GENERIC, "--owner-pubkey is not an ML-DSA-87 public key"));
    }
    let payout = match &args.payout_address {
        Some(a) => payload_of(a, prefix, "--payout-address")?,
        None => own_payout_payload_v1(&pubkey),
    };
    let payer = kaspa_addresses::Address::try_from(args.payer_address.as_str())
        .map_err(|e| err(exit::GENERIC, format!("--payer-address: {e}")))?;
    let timeout = Duration::from_secs(ctx.timeout_secs.clamp(2, 15));
    let mut urls: Vec<String> = ctx.rpc.iter().cloned().collect();
    urls.extend(args.quote_rpc.iter().cloned());
    urls.dedup();
    if urls.len() < 2 && !args.allow_single_rpc {
        return Err(err(
            exit::GENERIC,
            "two nodes or more (--rpc and --quote-rpc): one node's word is not a quote (--allow-single-rpc for your own)",
        ));
    }
    let mut nodes = Vec::new();
    for url in &urls {
        let node = snapshot::connect_to(&ctx.network, Some(url), timeout)
            .await
            .map_err(|(u, e)| err(exit::COMPONENT_DOWN, format!("{u}: {e}")))?;
        let status =
            node.node_status.as_ref().ok_or_else(|| err(exit::COMPONENT_DOWN, format!("{url} does not say which ruleset it runs")))?;
        if status.consensus_params_id != chain.ruleset || status.consensus_schedule_id != chain.schedule {
            return Err(err(
                exit::NETWORK_MISMATCH,
                format!(
                    "{url} runs another ruleset (params {} / schedule {}) than this build's ({} / {})",
                    status.consensus_params_id, status.consensus_schedule_id, chain.ruleset, chain.schedule
                ),
            ));
        }
        nodes.push(node);
    }
    // The funding: the same output on every node (a node that hides or invents it is outvoted, not believed).
    let mut picked: Option<(kaspa_consensus_core::tx::TransactionOutpoint, kaspa_consensus_core::tx::UtxoEntry)> = None;
    for node in &nodes {
        let f = crate::operator::model_remote::funding_of(node, &payer)
            .await
            .map_err(|_| err(exit::FUNDS, format!("{}: no spendable output at {payer}", node.url)))?;
        match &picked {
            None => picked = Some((f.outpoint, f.entry)),
            Some((o, e)) if *o == f.outpoint && e.amount == f.entry.amount => {}
            Some(_) => {
                return Err(err(
                    exit::GENERIC,
                    format!("the nodes do not agree on the payer's funding output ({}): nothing is quoted", node.url),
                ));
            }
        }
    }
    let (outpoint, entry) = picked.expect("at least one node");
    let now = nodes.iter().map(|n| n.daa()).min().unwrap_or(0);
    let possession = chain.params.palw_operator_id_unique_at(now);
    let funding = FundingInputV1 {
        txid: outpoint.transaction_id,
        index: outpoint.index,
        amount: entry.amount,
        block_daa_score: entry.block_daa_score,
        is_coinbase: entry.is_coinbase,
    };
    let build = |fee: u64| {
        build_bond_bundle_v1(
            &chain.params.net.to_string(),
            chain.domain,
            &chain.ruleset,
            &pubkey,
            args.collateral,
            payout,
            possession,
            &payer.to_string(),
            &entry.script_public_key,
            funding.clone(),
            fee,
            chain.floor,
            now + BOND_QUOTE_VALIDITY_DAA,
            urls.clone(),
        )
        .map_err(|e| err(exit::FUNDS, e.to_string()))
    };
    let draft = build(kaspa_pq_validator_core::ATTESTATION_TX_FEE_FLOOR_SOMPI)?;
    let fee = price(&chain, &draft)?;
    let bundle = build(fee)?;
    if args.out.exists() {
        return Err(err(exit::HOST, format!("{} exists: a bundle is never overwritten", args.out.display())));
    }
    std::fs::write(&args.out, bundle.to_json()).map_err(|e| err(exit::HOST, format!("{}: {e}", args.out.display())))?;
    let mode = ModeLabelV1::UnverifiedRemote;
    let summary = serde_json::json!({
        "schema": "misaka.bond.register-export.v1",
        "bundle": args.out.display().to_string(),
        "collateral_sompi": bundle.collateral, "floor_sompi": chain.floor, "fee_sompi": fee,
        "payout_payload": payout.to_string(), "operator_possession": possession, "expiry_daa": bundle.expiry_daa,
        "funding": format!("{}:{}", outpoint.transaction_id, outpoint.index),
        "nodes": urls, "mode": mode.as_str(), "mode_claim": mode.claim(),
        "next": format!("misaka bond register-sign {} --key-file <bond seed> --expect-collateral {} --max-fee-sompi {fee}", args.out.display(), bundle.collateral),
    });
    println!("{}", serde_json::to_string_pretty(&summary).unwrap_or_default());
    Ok(())
}

/// `misaka bond register-sign` (offline).
pub(crate) async fn sign(ctx: &crate::node::Ctx, args: SignArgs) -> CliResult {
    let chain = chain_of(ctx)?;
    let text = std::fs::read_to_string(&args.bundle).map_err(|e| err(exit::HOST, format!("{}: {e}", args.bundle.display())))?;
    let b = BondBundleV1::from_json(&text).map_err(|e| err(exit::GENERIC, e.to_string()))?;
    let owner = KeySigner(args.key.load_key()?);
    let payout = match &args.expect_payout {
        Some(a) => payload_of(a, chain.params.prefix(), "--expect-payout")?,
        None => own_payout_payload_v1(&owner.0.public_key()),
    };
    let policy = BondSignerPolicyV1 {
        network: chain.params.net.to_string(),
        network_domain: chain.domain,
        ruleset_id: chain.ruleset.clone(),
        collateral_floor_sompi: chain.floor,
        expect_collateral: args.expect_collateral,
        expect_payout: payout,
        max_fee_sompi: args.max_fee_sompi,
        now_daa: None,
    };
    // The class: an exported bundle's funding, DAA and possession flag are the nodes' word.
    let mode = ModeLabelV1::UnverifiedRemote;
    eprintln!("{}", mode.line());
    let accepted = args
        .accept_unverified_state
        .as_deref()
        .map(|t| ModeLabelV1::parse(t).ok_or_else(|| err(exit::GENERIC, format!("--accept-unverified-state {t:?} names no class"))))
        .transpose()?;
    signing_gate_v1(mode, accepted).map_err(|g| err(exit::NOT_READY, format!("E-MODE-UNVERIFIED: nothing was signed — {g}")))?;
    let owner_signed =
        bond_owner_sign_v1(&b, &policy, &owner).map_err(|e| err(exit::NOT_READY, format!("nothing was signed: {e}")))?;
    let payer = match &args.payer_key_file {
        Some(f) => KeySigner(crate::keys::KeySource { key_file: Some(f.clone()), key_stdin: false }.load_key()?),
        None => KeySigner(args.key.load_key()?),
    };
    let signed = bond_carrier_sign_v1(&owner_signed, &policy, &payer)
        .map_err(|e| err(exit::NOT_READY, format!("the carrier was not signed: {e}")))?;
    let out = args.out.clone().unwrap_or_else(|| args.bundle.with_extension("signed.json"));
    if out.exists() {
        return Err(err(exit::HOST, format!("{} exists: a signed file is never overwritten", out.display())));
    }
    std::fs::write(&out, signed.to_json()).map_err(|e| err(exit::HOST, format!("{}: {e}", out.display())))?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "schema": "misaka.bond.register-sign.v1", "signed": out.display().to_string(), "tx_id": signed.tx_id.to_string(),
            "bond": signed.bond, "mode": mode.as_str(),
            "note": "nothing was sent; the bond is NOT registered until a block carries it: misaka bond register-submit <file> --relay <node>,<node>",
        }))
        .unwrap_or_default()
    );
    Ok(())
}

/// `misaka bond register-submit`.
pub(crate) async fn submit(ctx: &crate::node::Ctx, args: SubmitArgs) -> CliResult {
    let chain = chain_of(ctx)?;
    let text = std::fs::read_to_string(&args.signed).map_err(|e| err(exit::HOST, format!("{}: {e}", args.signed.display())))?;
    let s = SignedBondRegistrationV1::from_json(&text).map_err(|e| err(exit::GENERIC, e.to_string()))?;
    let bond = verify_signed_bond_v1(&s, chain.domain, args.max_fee_sompi.unwrap_or(s.fee_sompi))
        .map_err(|e| err(exit::NOT_READY, format!("nothing was relayed: {e}")))?;
    let tx = s.transaction().map_err(|e| err(exit::GENERIC, e.to_string()))?;
    let timeout = Duration::from_secs(ctx.timeout_secs.clamp(2, 15));
    let min = args.relay_min.unwrap_or(args.relay.len() / 2 + 1).clamp(1, args.relay.len().max(1));
    let report = crate::operator::model_remote::relay(&chain.params.net.to_string(), &args.relay, &tx, min, timeout)
        .await
        .map_err(|e| err(exit::COMPONENT_DOWN, format!("too few nodes took the carrier: {e}")))?;
    let mode = ModeLabelV1::UnverifiedRemote;
    println!(
        "{}",
        serde_json::json!({ "event": "relayed", "tx_id": s.tx_id.to_string(), "bond": s.bond, "accepted_by": report.successes, "mode": mode.as_str(),
            "note": "an ACK is not inclusion" })
    );
    if args.no_wait {
        return Ok(());
    }
    // Follow the bond: registered once a node reports it under the key that signed it (the nodes' word, labelled).
    let deadline = std::time::Instant::now() + Duration::from_secs(20 * 60);
    while std::time::Instant::now() < deadline {
        for url in &args.relay {
            let Ok(node) = snapshot::connect_to(&ctx.network, Some(url), timeout).await else { continue };
            if let Ok(f) = node
                .client()
                .get_palw_producer_facts(chain.base_class.to_string(), bond.0.transaction_id.to_string(), bond.0.index, true)
                .await
                && f.bond_known
                && f.bond_registered_pubkey.eq_ignore_ascii_case(&s.pubkey)
            {
                println!(
                    "{}",
                    serde_json::json!({ "event": "registered", "bond": s.bond, "node": url, "mode": mode.as_str(),
                        "note": "reported by this node (UNVERIFIED_REMOTE): prove it with `misaka model verify`-style op-202 proofs against a block you pinned" })
                );
                return Ok(());
            }
        }
        tokio::time::sleep(Duration::from_secs(10)).await;
    }
    Err(err(
        exit::NOT_READY,
        format!("no node reports bond {} after 20 minutes: re-run register-submit (the same bytes are idempotent)", s.bond),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_pq_validator_core::ValidatorKey;

    fn ctx() -> crate::node::Ctx {
        crate::node::Ctx {
            output: crate::OutputFormat::Human,
            network: "testnet-12".into(),
            rpc: None,
            node_grpc: None,
            evm_rpc: String::new(),
            timeout_secs: 5,
            quiet: true,
            palw_drill_genesis_salt: None,
        }
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("misaka-bond-register-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A bundle as `register-export` writes it, for the owner seed `[1; 32]` paying from the payer seed `[2; 32]`.
    fn bundle(chain: &Chain, collateral: u64) -> BondBundleV1 {
        let owner = ValidatorKey::from_seed([1; 32]);
        let payer = ValidatorKey::from_seed([2; 32]);
        let payer_addr = payer.funding_address(chain.params.prefix());
        let mut b = build_bond_bundle_v1(
            &chain.params.net.to_string(),
            chain.domain,
            &chain.ruleset,
            owner.public_key(),
            collateral,
            own_payout_payload_v1(owner.public_key()),
            true,
            &payer_addr.to_string(),
            &kaspa_txscript::pay_to_address_script(&payer_addr),
            FundingInputV1 {
                txid: Hash64::from_u64_word(0xF0),
                index: 0,
                amount: collateral * 2,
                block_daa_score: 1,
                is_coinbase: false,
            },
            1_000_000,
            chain.floor,
            u64::MAX,
            vec!["a".into(), "b".into()],
        )
        .unwrap();
        b.fee_sompi = price(chain, &b).unwrap();
        b
    }

    fn sign_args(dir: &std::path::Path, b: &BondBundleV1, accept: Option<&str>) -> SignArgs {
        let path = dir.join("bond.json");
        std::fs::write(&path, b.to_json()).unwrap();
        let owner = dir.join("owner.seed").display().to_string();
        let payer = dir.join("payer.seed").display().to_string();
        if !dir.join("owner.seed").exists() {
            kaspa_pq_validator_core::write_validator_seed(&owner, &[1; 32]).unwrap();
            kaspa_pq_validator_core::write_validator_seed(&payer, &[2; 32]).unwrap();
        }
        SignArgs {
            bundle: path,
            key: crate::keys::KeySource { key_file: Some(owner), key_stdin: false },
            payer_key_file: Some(payer),
            expect_collateral: b.collateral,
            expect_payout: None,
            max_fee_sompi: b.fee_sompi,
            accept_unverified_state: accept.map(|s| s.to_string()),
            out: None,
        }
    }

    /// **`misaka bond register-sign` on files** (H1's NODELESS_BOND_REGISTRATION_ABSENT): an exported bundle is UNVERIFIED_REMOTE, so it
    /// signs only when the user names that class; then the owner and a DIFFERENT payer sign, the file verifies from its bytes and names the
    /// bond `<carrier>:0`; a moved payout or collateral signs nothing; a signed file is never overwritten.
    #[tokio::test]
    async fn register_sign_needs_the_class_named_and_refuses_a_moved_payout_or_collateral() {
        let chain = chain_of(&ctx()).unwrap();
        let b = bundle(&chain, chain.floor * 2);
        let dir = scratch("gate");
        assert!(sign(&ctx(), sign_args(&dir, &b, None)).await.is_err(), "no class named: nothing signed");
        assert!(!dir.join("bond.signed.json").exists());
        let dir = scratch("ok");
        let args = sign_args(&dir, &b, Some("UNVERIFIED_REMOTE"));
        sign(&ctx(), args).await.unwrap();
        let signed = SignedBondRegistrationV1::from_json(&std::fs::read_to_string(dir.join("bond.signed.json")).unwrap()).unwrap();
        let bond = verify_signed_bond_v1(&signed, chain.domain, signed.fee_sompi).unwrap();
        assert_eq!(signed.bond, format!("{}:0", bond.0.transaction_id));
        assert!(sign(&ctx(), sign_args(&dir, &b, Some("UNVERIFIED_REMOTE"))).await.is_err(), "never overwritten");
        let dir = scratch("payout");
        let moved = BondBundleV1 { payout_payload: Hash64::from_u64_word(0xEE), ..b.clone() };
        assert!(sign(&ctx(), sign_args(&dir, &moved, Some("UNVERIFIED_REMOTE"))).await.is_err());
        assert!(!dir.join("bond.signed.json").exists());
        let dir = scratch("collateral");
        let mut args = sign_args(&dir, &b, Some("UNVERIFIED_REMOTE"));
        args.expect_collateral = b.collateral + 1;
        assert!(sign(&ctx(), args).await.is_err());
        assert!(!dir.join("bond.signed.json").exists());
    }
}
