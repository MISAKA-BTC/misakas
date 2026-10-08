//! **RFC-0009 stage A0 — `misaka model add` without a node of your own**: quote → approve → re-read → sign → relay →
//! verify, on top of [`misaka_palw_remote::register`].
//!
//! The registrant's machine runs no `kaspad` and no Panel. It reads the chain from any node (`--rpc`), builds and signs
//! locally, and hands the finished carrier to several nodes (`--relay`). What it still needs is what the chain asks of
//! every registrant: an Active bond with room for the registration's exposure and its burn, and spendable BILI for the
//! carrier's fee. `--quote` stops after printing the quote: nothing is signed or sent.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;
use kaspa_consensus_core::tx::{Transaction, UtxoEntry};
use kaspa_rpc_core::api::rpc::RpcApi;
use misaka_palw_remote::register::{
    BondFactsV1, FilingV1, RegistrationFactsV1, RegistrationObservationV1, RegistrationQuoteV1, RegistrationTrackerV1, RegistryRowV1,
};
use misaka_palw_remote::relay::{RelayReport, Reply, fan_out_verdict, tx_id_of_bytes};
use std::time::Duration;

use crate::operator::finding::Finding;
use crate::operator::snapshot::NodeRead;
use crate::operator::tty::Halt;

/// An ML-DSA-87 signature's length: the probe carrier is priced with one of the real size, so the quote's fee is the
/// signed carrier's.
pub(crate) const MLDSA87_SIGNATURE_LEN: usize = 4627;

/// How long a quote stands, in DAA (about a minute at testnet-12's rate).
pub(crate) const QUOTE_VALIDITY_DAA: u64 = 600;

/// What `model add` takes for the node-less path.
#[derive(Clone, Debug, Default)]
pub(crate) struct RemoteArgs {
    /// Print the quote and stop (`--quote`): nothing is signed or sent.
    pub(crate) quote_only: bool,
    /// Relay the signed carrier through these nodes (`--relay a,b,c`) instead of the one `--rpc` names.
    pub(crate) relay: Vec<String>,
    /// How many of them must accept the carrier, and agree on its fate (`--relay-min`, default: a majority).
    pub(crate) relay_min: Option<usize>,
    /// The most the wallet may pay, in sompi (`--max-fee`); default: the quoted fee plus filings, exactly.
    pub(crate) max_wallet_sompi: Option<u64>,
    /// RFC-0009 A0 detached signing: write the UNSIGNED registration bundle here and stop (`--export-bundle`). No key is read.
    pub(crate) export_bundle: Option<std::path::PathBuf>,
    /// The bond's public key, hex (`--owner-pubkey`); the nodes' report is used (and labelled unverified) when absent.
    pub(crate) owner_pubkey: Option<String>,
    /// The address that pays the carrier (`--payer-address`) — it may belong to another key than the bond's.
    pub(crate) payer_address: Option<String>,
    /// More nodes to quote from (`--quote-rpc a,b`), beside `--rpc`: the terms must agree on all of them.
    pub(crate) quote_rpc: Vec<String>,
    /// Quote from ONE node (`--allow-single-rpc`): only for a node the operator runs; nothing cross-checks it.
    pub(crate) single_rpc: bool,
    /// A block hash the operator trusts (`--pin`): the owner bond and its key are proven against it and the proof rides in the bundle.
    pub(crate) pin: Option<String>,
    /// `--accept-unverified-state <LABEL>`: the class the user accepts signing in below VERIFIED_REMOTE.
    pub(crate) accept_unverified_state: Option<String>,
}

/// **The class `model add` signs in** (RFC-0009, 2026-10-08): a node on this machine is the user's own full node (`FULL_NODE`); a node
/// elsewhere, or a quorum of them, is `UNVERIFIED_REMOTE` (the owner key proven against `--pin` does not verify the terms, the funding or the
/// fork choice the registration is priced and folded on).
pub(crate) fn add_mode_v1(rpc_url: &str) -> misaka_palw_remote::verify::ModeLabelV1 {
    use misaka_palw_remote::verify::ModeLabelV1;
    let host = rpc_url.trim_start_matches("ws://").trim_start_matches("wss://").trim_start_matches("grpc://");
    let host = host.rsplit_once(':').map(|(h, _)| h).unwrap_or(host).trim_start_matches('[').trim_end_matches(']');
    if matches!(host, "127.0.0.1" | "localhost" | "::1") { ModeLabelV1::FullNode } else { ModeLabelV1::UnverifiedRemote }
}

/// Print the class and apply the gate (before the yes, before any key is read).
pub(crate) fn mode_gate_v1(
    flow: &mut crate::operator::tty::Flow,
    label: misaka_palw_remote::verify::ModeLabelV1,
    accept: Option<&str>,
) -> Result<(), crate::operator::tty::Halt> {
    use crate::operator::finding::{Finding, Severity};
    use misaka_palw_remote::verify::{ModeLabelV1, signing_gate_v1};
    flow.row(
        if label >= ModeLabelV1::VerifiedRemote { Severity::Ok } else { Severity::Warning },
        "mode",
        format!("{} — {}", label.as_str(), label.claim()),
    );
    let accepted = match accept {
        Some(t) => Some(ModeLabelV1::parse(t).ok_or_else(|| {
            crate::operator::tty::Halt::Blocked(
                Finding::error("E-MODE-LABEL", crate::exit::GENERIC, "--accept-unverified-state names no class")
                    .current(format!("{t:?}: HEADER_VERIFIED_FORK_CHOICE_UNVERIFIED or UNVERIFIED_REMOTE")),
            )
        })?),
        None => None,
    };
    signing_gate_v1(label, accepted).map_err(|g| {
        crate::operator::tty::Halt::Blocked(
            Finding::error(
                "E-MODE-UNVERIFIED",
                crate::exit::NOT_READY,
                "Nothing was signed: the state behind this registration is not verified",
            )
            .current(g.to_string())
            .reason("not running a full node is fine; trusting what a node says is not — the class is named so the choice is yours")
            .fix(format!("run against your own node, or --accept-unverified-state {}", label.as_str())),
        )
    })
}

impl RemoteArgs {
    pub(crate) fn active(&self) -> bool {
        self.quote_only || !self.relay.is_empty() || self.export_bundle.is_some()
    }

    pub(crate) fn min_agree(&self) -> usize {
        self.relay_min.unwrap_or(self.relay.len() / 2 + 1).clamp(1, self.relay.len().max(1))
    }
}

fn blocked(code: &'static str, what: impl Into<String>, why: impl Into<String>) -> Halt {
    Halt::Blocked(Finding::error(code, crate::exit::COMPONENT_DOWN, what.into()).current(why.into()))
}

fn parse_u128(s: &str) -> u128 {
    s.trim().parse().unwrap_or(0)
}

/// The identity of the bytes the bond key signs: the registration object as built, signature empty.
pub(crate) fn object_digest(unsigned: &PalwConsensusObjectV2) -> Hash64 {
    kaspa_consensus_core::palw_model_registration_v1::palw_registration_object_id_v1(&borsh::to_vec(unsigned).unwrap_or_default())
}

/// The live terms' digest: what a quote was read under. Any change in what the node reports is a different digest.
async fn terms_digest(node: &NodeRead) -> Result<Hash64, Halt> {
    let r = node
        .client()
        .get_palw_registration_terms()
        .await
        .map_err(|e| blocked("E-NODE-TERMS", "The registration terms could not be read", e.to_string()))?;
    // **Without `tip_daa`**: it is the node's clock, not a term. Hashed in, the digest moved with every block, so the pre-sign
    // gate stopped on "terms changed" whenever a block arrived between the quote and the signature, and no two nodes at
    // different tips could ever agree on a quote.
    let mut r = r;
    r.tip_daa = 0;
    let bytes = serde_json::to_vec(&r).unwrap_or_default();
    let mut s = blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/remote/registration-terms/v1").to_state();
    s.update(&bytes);
    let mut out = [0u8; 64];
    out.copy_from_slice(s.finalize().as_bytes());
    Ok(Hash64::from_bytes(out))
}

/// The funding the carrier would spend, and the probe carrier priced at the signed object's size.
pub(crate) struct Funding {
    pub(crate) outpoint: kaspa_consensus_core::tx::TransactionOutpoint,
    pub(crate) entry: UtxoEntry,
    pub(crate) spendable: u64,
}

pub(crate) async fn funding(node: &NodeRead, key: &kaspa_pq_validator_core::ValidatorKey) -> Result<Funding, Halt> {
    funding_of(node, &key.funding_address(node.nv.params.prefix())).await
}

/// The funding of an ADDRESS: the payer's key is not needed to read it.
pub(crate) async fn funding_of(node: &NodeRead, addr: &kaspa_addresses::Address) -> Result<Funding, Halt> {
    let candidates = crate::palw_fp::lifecycle_candidates_v1(&node.nv, addr)
        .await
        .map_err(|e| blocked("E-FUNDS-UNREAD", "The funding wallet could not be read", e.msg))?;
    let spendable = candidates.iter().map(|(_, e)| e.amount).sum();
    let (outpoint, entry) = candidates.first().cloned().ok_or_else(|| {
        Halt::Blocked(
            Finding::error("E-FUNDS-NONE", crate::exit::FUNDS, "No spendable output funds the registration carrier")
                .current(format!("{addr}: no mature, unbonded, unspent output"))
                .fix("send BILI to this address — the carrier's fee is paid from it"),
        )
    })?;
    Ok(Funding { outpoint, entry, spendable })
}

/// **Price a carrier without a key**: the unsigned body plus a signature script of the real length. The same mass and fee as
/// `palw_fp::build_carrier_priced_v1` (which signs to measure) — pinned by a test.
pub(crate) fn price_carrier(
    params: &kaspa_consensus_core::config::params::Params,
    object: &PalwConsensusObjectV2,
    funding_outpoint: kaspa_consensus_core::tx::TransactionOutpoint,
    funding_entry: &UtxoEntry,
) -> Result<(u64, u64), String> {
    use kaspa_consensus_core::mass::MassCalculator;
    let calc = MassCalculator::new(
        params.mass_per_tx_byte,
        params.mass_per_script_pub_key_byte,
        params.mass_per_sig_op,
        params.storage_mass_parameter,
    );
    let floor = kaspa_pq_validator_core::ATTESTATION_TX_FEE_FLOOR_SOMPI;
    let probe = misaka_palw_remote::bundle::carrier_body_v1(
        object,
        funding_outpoint,
        funding_entry.amount,
        floor,
        &funding_entry.script_public_key,
        misaka_palw_remote::bundle::placeholder_funding_script_v1(),
    )
    .map_err(|e| format!("build the carrier: {e}"))?;
    let compute_mass = calc.calc_non_contextual_masses(&probe).compute_mass;
    let fee =
        kaspa_pq_validator_core::relay_fee_for_compute_mass(compute_mass).max(floor).max(crate::palw_fp::carrier_rent_v1(object));
    if funding_entry.amount <= fee {
        return Err(format!("the funding holds {} sompi, under its {fee} sompi fee", funding_entry.amount));
    }
    Ok((compute_mass, fee))
}

/// **Read the facts a quote is made of**, from `node`, for `unsigned` (and `probe`, the same object with a signature of the
/// real length, which prices the carrier). **No key is needed**: the owner's PUBLIC key and the payer's ADDRESS are enough; what
/// the chain reports about them is [`UNVERIFIED_REMOTE_STATE`](misaka_palw_remote::bundle::UNVERIFIED_REMOTE_STATE).
#[allow(clippy::too_many_arguments)]
pub(crate) async fn read_facts(
    node: &NodeRead,
    params: &kaspa_consensus_core::config::params::Params,
    bundle: &kaspa_consensus_core::palw_mode_v2::PalwConsensusParamsV2,
    owner_pubkey: &[u8],
    payer: &kaspa_addresses::Address,
    bond: &str,
    bond_op: kaspa_consensus_core::tx::TransactionOutpoint,
    unsigned: &PalwConsensusObjectV2,
    probe: &PalwConsensusObjectV2,
    sponsor: Option<u64>,
) -> Result<(RegistrationFactsV1, Funding), Halt> {
    let PalwConsensusObjectV2::ClassRegistered { class_id, artifact_root, .. } = unsigned else {
        return Err(blocked("E-MODEL-REGISTRATION", "Not a registration", "the object is not ClassRegistered"));
    };
    let dag = node
        .client()
        .get_block_dag_info()
        .await
        .map_err(|e| blocked("E-NODE-DAG", "The node's tip could not be read", e.to_string()))?;
    let claims = node
        .client()
        .get_palw_claims(bond.to_string(), "seat".into(), false, 1)
        .await
        .map_err(|e| blocked("E-SETUP-BOND-UNREAD", "The bond could not be read", e.to_string()))?;
    let facts = node
        .client()
        .get_palw_producer_facts(String::new(), bond_op.transaction_id.to_string(), bond_op.index, true)
        .await
        .map_err(|e| blocked("E-SETUP-BOND-UNREAD", "The bond's exposure could not be read", e.to_string()))?;
    // The ledger the fold applies the ceiling to: the committed ledger past palw_rcore_plus (an older node reports the
    // reserved one in its place), plus what the bond holds as an accuser.
    let backing =
        parse_u128(&facts.bond_committed).max(parse_u128(&facts.bond_reserved_exposure)) + parse_u128(&facts.bond_accuser_exposure);
    let fund = funding_of(node, payer).await?;
    let (mass, fee) = price_carrier(params, probe, fund.outpoint, &fund.entry)
        .map_err(|e| Halt::Blocked(Finding::error("E-FUNDS-SHORT", crate::exit::FUNDS, "The carrier cannot be funded").current(e)))?;
    let daa = dag.virtual_daa_score;
    let facts = RegistrationFactsV1 {
        network_domain: kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
            params.net.to_string().as_bytes(),
            Some(params.genesis.hash),
        ),
        terms_digest: terms_digest(node).await?,
        tip_hash: dag.sink,
        tip_daa: daa,
        class_id: *class_id,
        artifact_root: *artifact_root,
        object_digest: object_digest(unsigned),
        exposure_sompi: bundle.state.registration_exposure_sompi(),
        burn_sompi: if params.palw_audit_2026_09_23_active_at(daa) {
            kaspa_consensus_core::palw_state_v2::PALW_CLASS_REGISTRATION_BURN_SOMPI_V1
        } else {
            0
        },
        bond: BondFactsV1 {
            outpoint: bond.to_string(),
            known: claims.bond_known,
            key_matches: claims.bond_pubkey.eq_ignore_ascii_case(&faster_hex::hex_string(owner_pubkey)),
            retiring: claims.bond_retiring_since_daa.is_some(),
            collateral_sompi: claims.bond_collateral,
            backing_sompi: backing,
            live_locked_sompi: parse_u128(&claims.bond_live_locked_sompi),
        },
        carrier_mass: mass,
        carrier_fee_sompi: fee,
        wallet_spendable_sompi: fund.spendable,
        filings: sponsor
            .map(|s| {
                vec![FilingV1 {
                    what: "Activation Pool sponsor (a separate carrier, after the registration folds)".into(),
                    sompi: s,
                    from_wallet: true,
                }]
            })
            .unwrap_or_default(),
    };
    Ok((facts, fund))
}

/// What the quoting nodes reported beyond the quote's figures: the ruleset each runs.
#[derive(Clone, Debug, Default)]
pub(crate) struct NodeReport {
    pub(crate) node: String,
    pub(crate) consensus_params_id: String,
}

/// **The same quote from several independent nodes**, each checked against this build's genesis and ruleset. A node that cannot
/// answer is silence (reported); the agreement itself is `agree_quote_facts_v1`'s. The chosen funding output must be the same on
/// every node. Nothing here is proven against a header: the result is `UNVERIFIED_REMOTE_STATE`.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn read_facts_many(
    ctx: &crate::node::Ctx,
    primary: &NodeRead,
    extra_urls: &[String],
    params: &kaspa_consensus_core::config::params::Params,
    bundle: &kaspa_consensus_core::palw_mode_v2::PalwConsensusParamsV2,
    owner_pubkey: &[u8],
    payer: &kaspa_addresses::Address,
    bond: &str,
    bond_op: kaspa_consensus_core::tx::TransactionOutpoint,
    unsigned: &PalwConsensusObjectV2,
    probe: &PalwConsensusObjectV2,
    sponsor: Option<u64>,
    silent: &mut Vec<String>,
) -> Result<(Vec<(String, RegistrationFactsV1)>, Funding, Vec<NodeReport>), Halt> {
    let salt = ctx
        .palw_drill_genesis_salt
        .as_deref()
        .map(kaspa_consensus_core::config::drill::PalwDrillSaltV1::from_hex)
        .transpose()
        .ok()
        .flatten();
    let timeout = Duration::from_secs(ctx.timeout_secs.clamp(2, 15));
    let ours = params.consensus_params_id().to_string();
    let mut answers = Vec::new();
    let mut reports = Vec::new();
    let mut chosen: Option<Funding> = None;
    let mut others: Vec<NodeRead> = Vec::new();
    for url in extra_urls {
        match crate::operator::snapshot::connect_to(&ctx.network, Some(url), timeout).await {
            Ok(n) if n.ops_0122 && n.url != primary.url => others.push(n),
            Ok(n) if n.url == primary.url => {}
            Ok(n) => silent.push(format!("{}: predates the registration reads", n.url)),
            Err((u, e)) => silent.push(format!("{u}: {e}")),
        }
    }
    for node in std::iter::once(primary).chain(others.iter()) {
        // The genesis this tool signs for, and the ruleset this build derived: a node on another one is not a quote source.
        if let Some(status) = &node.node_status {
            if let Err(e) = crate::wallet::node_genesis_verdict(params, salt.as_ref(), status) {
                return Err(blocked("E-NODE-GENESIS", format!("{} runs another genesis than this CLI signs for", node.url), e.msg));
            }
            if !status.consensus_params_id.is_empty() && status.consensus_params_id != ours {
                return Err(blocked(
                    "E-NODE-RULESET",
                    format!("{} runs another ruleset than this build", node.url),
                    format!("node {} · this build {}", status.consensus_params_id, ours),
                ));
            }
        }
        let (facts, fund) = read_facts(node, params, bundle, owner_pubkey, payer, bond, bond_op, unsigned, probe, sponsor).await?;
        match &chosen {
            None => chosen = Some(fund),
            Some(first) if first.outpoint != fund.outpoint || first.entry.amount != fund.entry.amount => {
                return Err(blocked(
                    "E-QUOTE-DISAGREE",
                    "The nodes disagree on the funding output",
                    format!(
                        "{} chose {}:{} ({} sompi), another node chose {}:{} ({} sompi)",
                        node.url,
                        fund.outpoint.transaction_id,
                        fund.outpoint.index,
                        fund.entry.amount,
                        first.outpoint.transaction_id,
                        first.outpoint.index,
                        first.entry.amount
                    ),
                ));
            }
            Some(_) => {}
        }
        reports.push(NodeReport {
            node: node.url.clone(),
            consensus_params_id: node.node_status.as_ref().map(|s| s.consensus_params_id.clone()).unwrap_or_default(),
        });
        answers.push((node.url.clone(), facts));
    }
    Ok((answers, chosen.ok_or_else(|| blocked("E-FUNDS-NONE", "No node answered", "no funding output"))?, reports))
}

/// Print a quote.
pub(crate) fn show(flow: &crate::operator::tty::Flow, quote: &RegistrationQuoteV1) {
    flow.ui.say("");
    flow.ui.say(&crate::operator::finding::paint::bold("  Quote (RFC-0009 §3.4) — what this registration costs, by payer:"));
    for line in quote.lines() {
        flow.ui.sub(&line);
    }
    flow.ui.sub(&format!("quote      {}", quote.digest()));
}

/// **Relay one signed carrier through every node in `urls`**; success needs `min` of them to return our own id.
pub(crate) async fn relay(
    network: &str,
    urls: &[String],
    tx: &Transaction,
    min: usize,
    timeout: Duration,
) -> Result<RelayReport, String> {
    let expected = tx_id_of_bytes(tx);
    let mut replies = Vec::with_capacity(urls.len());
    for url in urls {
        let reply = match crate::operator::snapshot::connect_to(network, Some(url), timeout).await {
            Err((u, e)) => Reply::Unreachable(format!("{u}: {e}")),
            Ok(node) => match node.client().submit_transaction(tx.as_ref().into(), false).await {
                Ok(id) => Reply::Accepted(id),
                Err(e) => {
                    let text = e.to_string();
                    // Our own bytes already held (an earlier send, another relay's gossip): idempotent success.
                    if text.contains("already") && text.contains(&expected.to_string()) {
                        Reply::AlreadyKnown(expected)
                    } else {
                        Reply::Refused(text)
                    }
                }
            },
        };
        replies.push((url.clone(), reply));
    }
    fan_out_verdict(expected, replies, min).map_err(|e| e.to_string())
}

/// One round of observations from every relay node.
pub(crate) async fn observe(
    network: &str,
    urls: &[String],
    class_hex: &str,
    object_id: &str,
    txid: &str,
    timeout: Duration,
) -> Vec<RegistrationObservationV1> {
    let mut out = Vec::with_capacity(urls.len());
    for url in urls {
        let mut o = RegistrationObservationV1 {
            node: url.clone(),
            in_mempool: false,
            included: None,
            row: None,
            refused: None,
            accepted_daa: None,
        };
        if let Ok(node) = crate::operator::snapshot::connect_to(network, Some(url), timeout).await {
            let r = crate::palw_model_ops::track_after_submit(node.client(), class_hex, object_id, txid, None).await;
            o.in_mempool = r.mempool_accepted && !r.included;
            if r.included {
                o.included = Some((r.included_block.parse().unwrap_or_default(), r.included_daa));
            }
            if r.reject_code
                == kaspa_consensus_core::palw_model_registration_v1::PalwModelRegistrationCodeV1::RegistrationDropped.code()
            {
                o.refused = Some(if r.drop_reason.is_empty() {
                    r.reject_code.clone()
                } else {
                    format!("{}: {}", r.reject_code, r.drop_reason)
                });
            }
            if let Ok(table) = node.client().get_palw_classes().await
                && let Some(row) = table.classes.into_iter().find(|c| c.class_id == class_hex)
            {
                o.accepted_daa = Some(row.registered_daa);
                o.row = Some(RegistryRowV1 {
                    class_id: row.class_id.parse().unwrap_or_default(),
                    artifact_root: row.artifact_root.parse().unwrap_or_default(),
                    registrant_bond: None,
                    lifecycle: row.status,
                });
            }
        }
        out.push(o);
    }
    out
}

/// A tracker for a relayed registration.
pub(crate) fn tracker(tx: &Transaction, unsigned: &PalwConsensusObjectV2, bond: &str, min_agree: usize) -> RegistrationTrackerV1 {
    let (class_id, root) = match unsigned {
        PalwConsensusObjectV2::ClassRegistered { class_id, artifact_root, .. } => (*class_id, *artifact_root),
        _ => (Hash64::default(), Hash64::default()),
    };
    RegistrationTrackerV1::new(tx_id_of_bytes(tx), object_digest(unsigned), class_id, root, bond.to_string(), min_agree)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_relay_quorum_defaults_to_a_majority_and_never_exceeds_the_nodes_named() {
        let a =
            |n: usize, min: Option<usize>| RemoteArgs { relay: vec!["x".into(); n], relay_min: min, ..Default::default() }.min_agree();
        assert_eq!(a(3, None), 2);
        assert_eq!(a(4, None), 3);
        assert_eq!(a(1, None), 1);
        assert_eq!(a(2, Some(5)), 2);
        assert_eq!(a(2, Some(0)), 1);
        assert!(!RemoteArgs::default().active());
        assert!(RemoteArgs { quote_only: true, ..Default::default() }.active());
        // RFC-0009 modes: a node on this machine is the user's own full node; anything else is the nodes' word.
        use misaka_palw_remote::verify::ModeLabelV1;
        for own in ["127.0.0.1:16110", "ws://localhost:17110", "[::1]:16110"] {
            assert_eq!(add_mode_v1(own), ModeLabelV1::FullNode, "{own}");
        }
        for remote in ["203.0.113.5:16110", "ws://node.example:17110", "10.0.0.2:16110"] {
            assert_eq!(add_mode_v1(remote), ModeLabelV1::UnverifiedRemote, "{remote}");
        }
    }
}
