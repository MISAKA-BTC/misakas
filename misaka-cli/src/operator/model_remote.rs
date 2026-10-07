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
}

impl RemoteArgs {
    pub(crate) fn active(&self) -> bool {
        self.quote_only || !self.relay.is_empty()
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
    let addr = key.funding_address(node.nv.params.prefix());
    let candidates = crate::palw_fp::lifecycle_candidates_v1(&node.nv, &addr)
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

/// **Read the facts a quote is made of**, from `node`, for `unsigned` (and `probe`, the same object with a signature of the
/// real length, which prices the carrier).
#[allow(clippy::too_many_arguments)]
pub(crate) async fn read_facts(
    node: &NodeRead,
    params: &kaspa_consensus_core::config::params::Params,
    bundle: &kaspa_consensus_core::palw_mode_v2::PalwConsensusParamsV2,
    key: &kaspa_pq_validator_core::ValidatorKey,
    bond: &str,
    bond_op: kaspa_consensus_core::tx::TransactionOutpoint,
    unsigned: &PalwConsensusObjectV2,
    probe: &PalwConsensusObjectV2,
    sponsor: Option<u64>,
) -> Result<RegistrationFactsV1, Halt> {
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
    let fund = funding(node, key).await?;
    let (_, mass, fee) = crate::palw_fp::build_carrier_priced_v1(key, &node.nv, probe, fund.outpoint, &fund.entry)
        .map_err(|e| Halt::Blocked(Finding::error("E-FUNDS-SHORT", crate::exit::FUNDS, "The carrier cannot be funded").current(e)))?;
    let daa = dag.virtual_daa_score;
    Ok(RegistrationFactsV1 {
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
            key_matches: claims.bond_pubkey.eq_ignore_ascii_case(&faster_hex::hex_string(key.public_key())),
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
    })
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
    }
}
