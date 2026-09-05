//! **ADR-0077 Decision 3 — the gateway reads the chain it commits to.**
//!
//! Before this module the gateway read a hand-refreshed `anchor.json` and an `identity.json` whose
//! `class_id` nothing checked (ADR-0075 §4 records the gap): the class could name a row no network
//! knows, on a lane no network certifies, under a bond with no room, and every one of those facts
//! would first be learned by the chain, at the transition, after a fee. Decision 3 moves all four
//! reads to the entrance and gives them names:
//!
//! ```text
//!   registered      the class registry row exists on THIS network
//!   fp_certified    the class is seated on the free-prompt lane (ADR-0075 ClassLaneCertified,
//!                   the genesis set ∪ the chain set)
//!   bond_active     the executor bond is known and may produce
//!   exposure_room   ceiling − reserved: room for one more claim
//! ```
//!
//! **An uncertified class ANSWERS and never submits.** That is the whole shape of R0's "the only
//! reasons a commitment does not reach the chain are the chain's — never a runtime mode": the
//! inference runs, the user gets the product, the capture and the roots exist, and the commitment
//! carries the chain-side reason it stays in the outbox. `commit_refusal` is the one hook, shared
//! with SA-1's budget refusals so a caller sees ONE sentence for "this answer did not become a
//! claim, and here is why".
//!
//! **Two sources, and the difference is honest.** With `--rpc` the four facts and the anchor come
//! from the node, per job. With `--anchor <json>` alone (the devnet drills, the smoke script, an
//! air-gapped rehearsal) the anchor comes from a file and the four facts are UNKNOWN — reported as
//! unknown, never as true. That is not a hole in Decision 3: a gateway with no RPC endpoint also
//! has no way to submit, so "it must not submit on facts it does not have" is satisfied by
//! construction rather than by a check.

use std::path::{Path, PathBuf};

use kaspa_hashes::Hash64;

/// The four facts Decision 3 names, plus the anchor they are read at.
///
/// `Default` is the shape of "nothing was read": every gate false, every number zero. Every field
/// that could be read as permission is false by default, because the failure mode this whole
/// module exists to close is a gateway that treated an unknown as a yes.
#[derive(Clone, Debug, Default)]
pub struct ChainFacts {
    /// Where the facts came from, in one phrase — printed in `/health` so an operator can tell a
    /// live read from a file at a glance.
    pub source: String,
    /// True when a node answered at all. False is not a fault: the answer still goes out.
    pub live: bool,
    pub chain_point: String,
    pub daa_score: u64,
    /// Decision 3's `registered`: this network knows the class.
    pub registered: bool,
    /// Decision 3's `fp_certified`: the class is seated on the free-prompt lane.
    pub fp_certified: bool,
    /// Decision 3's `bond_active`: the bond is known AND has no reason it may not produce.
    pub bond_active: bool,
    /// Why the bond may not produce, in the chain's own words. Empty when it may.
    pub bond_not_ready_reason: String,
    pub bond_collateral: u64,
    /// Decision 3's `exposure_room`: `ceiling − reserved`, in sompi.
    pub exposure_room_sompi: u64,
    /// The bond's whole exposure ceiling, before anything it already holds is taken out.
    ///
    /// Carried BESIDE the room because the two answer different questions and one of them was
    /// being asked with the other's number: "may this claim fit right now" is the room's, and "how
    /// much of this bond may public jobs use in a day" is the ceiling's. Budgeting the day against
    /// the room charges every open claim twice — once when the chain reserved it, again in the
    /// gateway's own counter — and a bond sized for four claims then stopped at two.
    pub bond_exposure_ceiling_sompi: u64,
    /// What the chain says ONE claim of this class reserves. The gateway's own
    /// `--claim-exposure-sompi` overrides it; without one this is the price used, so an operator
    /// who configured a node correctly does not also have to configure a number the node knows.
    pub claim_exposure_sompi: u64,
    /// Zero means this network prices no free-prompt lane at all, and a commitment on it would
    /// never enter the state — a refusal with a name, not a silent no-op.
    pub fp_quanta_per_canonical_job: u32,
    pub fp_max_quanta_per_receipt: u32,
    /// **ADR-0082 Decisions 10/11: has this network armed `Params::palw_fp_decode_rules`?**
    ///
    /// `false` — every shipped preset, and every node that does not report it — means the lane is
    /// greedy: a request asking for a temperature is refused with the fence's name rather than
    /// answered greedily under a job id that says otherwise.
    ///
    /// It is a CHAIN fact and not a gateway flag on purpose. A flag could disagree with the
    /// network, and the direction it would disagree in is the expensive one: the operator turns
    /// sampling "on", every commitment the gateway files is refused by the transition
    /// (`SamplingNotArmed`), and the exposure has already been spent on the inference.
    ///
    /// Today no node reports it — `GetPalwProducerFactsResponse` has no such field — so this stays
    /// `false` on every read. That is the correct answer on every shipped preset; when the RPC
    /// grows the field, the mapping in `RpcChainSource::read` is one line.
    pub fp_decode_rules_armed: bool,
    /// The freshness binding, from the node's sink (`--rpc`) or from `anchor.json`.
    pub anchor_block: Hash64,
    pub anchor_daa: u64,
    /// Set when a live read was attempted and failed. A gateway that cannot reach its node answers
    /// and does not commit, and says which of the two happened.
    pub read_error: Option<String>,
}

impl ChainFacts {
    /// **The chain-side reasons a commitment does not leave the outbox** (Decision 3), by name and
    /// in the order an operator can act on them. `None` means the chain has no objection — it does
    /// NOT mean the job may commit, because SA-1's budget still gets a veto.
    ///
    /// A source that read nothing (the `--anchor`-only form) objects to nothing here: it also
    /// submits nothing, so there is no unknown being read as a yes.
    pub fn commit_refusal(&self) -> Option<String> {
        if !self.live {
            return None;
        }
        if let Some(e) = &self.read_error {
            return Some(format!("the node could not be read for this job ({e}) — answered, not committed"));
        }
        if !self.registered {
            return Some(
                "this network does not know this class (`registered` is false in /health) — the answer is the product, \
                         but no commitment on an unregistered class can enter the state"
                    .to_string(),
            );
        }
        if !self.fp_certified {
            return Some(
                "this class is not seated on the free-prompt lane (ADR-0075 ClassLaneCertified; `fp_certified` is false \
                         in /health) — a commitment would be refused as FreePromptLaneUncertified"
                    .to_string(),
            );
        }
        if self.fp_quanta_per_canonical_job == 0 {
            return Some(
                "this network prices no free-prompt lane (`fp_quanta_per_canonical_job` is zero) — a commitment here \
                         enters no state"
                    .to_string(),
            );
        }
        if !self.bond_active {
            let why = if self.bond_not_ready_reason.is_empty() {
                "the chain does not know this bond".to_string()
            } else {
                self.bond_not_ready_reason.clone()
            };
            return Some(format!("the executor bond is not producible: {why} (`bond_active` is false in /health)"));
        }
        if self.exposure_room_sompi == 0 {
            return Some(
                "the bond's exposure ceiling leaves no room for another claim (`exposure_room` is zero in /health) — \
                         ADR-0077 Decision 4: answered and queued, not submitted and refused at the transition"
                    .to_string(),
            );
        }
        None
    }

    /// Were these facts actually READ? A source with no node, and a node that could not be
    /// reached, are both "I do not know" — and the second one is the trap: a failed read leaves
    /// every gate at its `false` default, which reads exactly like a chain that answered no.
    fn known(&self) -> bool {
        self.live && self.read_error.is_none()
    }

    /// What `/health` says. All four names appear in every answer, including the unknown one —
    /// a field that disappears when it is unknown is a field an operator reads as fine, and a
    /// `false` where the truth is "unreachable" is a field an operator acts on wrongly.
    pub fn health_json(&self) -> serde_json::Value {
        serde_json::json!({
            "source": self.source,
            "live": self.live,
            "read_error": self.read_error,
            "chain_point": self.chain_point,
            "daa_score": self.daa_score,
            "registered": if self.known() { serde_json::json!(self.registered) } else { serde_json::json!("unknown") },
            "fp_certified": if self.known() { serde_json::json!(self.fp_certified) } else { serde_json::json!("unknown") },
            "bond_active": if self.known() { serde_json::json!(self.bond_active) } else { serde_json::json!("unknown") },
            "bond_not_ready_reason": self.bond_not_ready_reason,
            "exposure_room": if self.known() { serde_json::json!(self.exposure_room_sompi) } else { serde_json::json!("unknown") },
            "bond_collateral": self.bond_collateral,
            "chain_claim_exposure_sompi": self.claim_exposure_sompi,
            "fp_quanta_per_canonical_job": self.fp_quanta_per_canonical_job,
            "fp_max_quanta_per_receipt": self.fp_max_quanta_per_receipt,
            "fp_decode_rules_armed": self.fp_decode_rules_armed,
            "anchor_daa": self.anchor_daa,
        })
    }
}

/// The anchor file the drills and the smoke script refresh by hand — Decision 3's predecessor,
/// kept as the offline form and never as the default.
#[derive(serde::Deserialize)]
struct AnchorFile {
    /// 64-byte hex: a recent chain block — the freshness binding.
    anchor_block: String,
    anchor_daa: u64,
}

fn hash64_from_hex(s: &str, what: &str) -> Result<Hash64, String> {
    let mut out = [0u8; 64];
    if s.len() != 128 || faster_hex::hex_decode(s.as_bytes(), &mut out).is_err() {
        return Err(format!("{what} is not 128 hex chars"));
    }
    Ok(Hash64::from_bytes(out))
}

pub fn read_anchor_file(path: &Path) -> Result<(Hash64, u64), String> {
    let raw = std::fs::read_to_string(path).map_err(|e| format!("cannot read anchor file {}: {e}", path.display()))?;
    let file: AnchorFile = serde_json::from_str(&raw).map_err(|e| format!("anchor file is not valid JSON: {e}"))?;
    Ok((hash64_from_hex(&file.anchor_block, "anchor_block")?, file.anchor_daa))
}

/// Where a job's chain facts come from.
pub enum ChainSource {
    /// Decision 3's form: the node, over wRPC-borsh, per job.
    Rpc(RpcChainSource),
    /// The offline form: an anchor file and four unknowns.
    AnchorFile(PathBuf),
}

impl ChainSource {
    pub fn read(&self) -> ChainFacts {
        match self {
            Self::Rpc(rpc) => rpc.read(),
            Self::AnchorFile(path) => {
                let mut facts = ChainFacts { source: format!("anchor file {}", path.display()), live: false, ..Default::default() };
                match read_anchor_file(path) {
                    Ok((block, daa)) => {
                        facts.anchor_block = block;
                        facts.anchor_daa = daa;
                    }
                    Err(e) => facts.read_error = Some(e),
                }
                facts
            }
        }
    }

    /// Whether this source can submit at all. The `--anchor`-only form cannot, which is why it is
    /// allowed to have four unknowns.
    pub fn can_submit(&self) -> bool {
        matches!(self, Self::Rpc(_))
    }
}

/// **The node, asked fresh per job.**
///
/// A connection is made per read rather than kept: the read happens once per inference, an
/// inference is seconds of a whole model, and a loopback wRPC connect is milliseconds. Holding a
/// long-lived client in a std-threads process would buy nothing measurable and would add the one
/// failure mode this lane cannot afford — a stale connection that answers with yesterday's facts.
pub struct RpcChainSource {
    runtime: tokio::runtime::Runtime,
    url: String,
    class_id: String,
    bond_txid: String,
    bond_index: u32,
    timeout: std::time::Duration,
}

impl RpcChainSource {
    /// `endpoint` is `host:port` or a full `ws://…` URL — the same `--rpc` spelling `misaka` takes.
    pub fn new(endpoint: &str, class_id: String, bond_txid: String, bond_index: u32, timeout_secs: u64) -> Result<Self, String> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .map_err(|e| format!("cannot start the RPC runtime: {e}"))?;
        Ok(Self {
            runtime,
            url: normalize_endpoint(endpoint),
            class_id,
            bond_txid,
            bond_index,
            timeout: std::time::Duration::from_secs(timeout_secs.clamp(1, 30)),
        })
    }

    pub fn read(&self) -> ChainFacts {
        let mut facts = ChainFacts { source: format!("rpc {}", self.url), live: true, ..Default::default() };
        match self.runtime.block_on(self.read_async()) {
            Ok(read) => {
                let (dag, producer) = read;
                facts.chain_point = producer.chain_point.clone();
                facts.daa_score = producer.daa_score;
                facts.anchor_block = match hash64_from_hex(&dag.0, "the node's sink") {
                    Ok(h) => h,
                    Err(e) => {
                        facts.read_error = Some(e);
                        return facts;
                    }
                };
                facts.anchor_daa = dag.1;
                facts.registered = producer.available;
                facts.fp_certified = producer.fp_certified;
                facts.bond_active = producer.bond_known && producer.not_ready_reason.is_empty();
                facts.bond_not_ready_reason = producer.not_ready_reason.clone();
                facts.bond_collateral = producer.bond_collateral;
                facts.exposure_room_sompi = exposure_room(&producer.bond_exposure_ceiling, &producer.bond_reserved_exposure);
                facts.bond_exposure_ceiling_sompi = decimal_u128(&producer.bond_exposure_ceiling).min(u64::MAX as u128) as u64;
                facts.claim_exposure_sompi = decimal_u128(&producer.bond_claim_exposure).min(u64::MAX as u128) as u64;
                facts.fp_quanta_per_canonical_job = producer.fp_quanta_per_canonical_job;
                facts.fp_max_quanta_per_receipt = producer.fp_max_quanta_per_receipt;
            }
            Err(e) => facts.read_error = Some(e),
        }
        facts
    }

    /// The two calls, in one connection: the sink and its DAA (the anchor Decision 3 says must be
    /// fresh), and the producer facts (the other three names).
    async fn read_async(&self) -> Result<((String, u64), kaspa_rpc_core::GetPalwProducerFactsResponse), String> {
        use kaspa_rpc_core::api::rpc::RpcApi;
        use kaspa_wrpc_client::{
            KaspaRpcClient, WrpcEncoding,
            client::{ConnectOptions, ConnectStrategy},
        };
        let client = KaspaRpcClient::new(WrpcEncoding::Borsh, Some(&self.url), None, None, None).map_err(|e| e.to_string())?;
        let options = ConnectOptions {
            block_async_connect: true,
            connect_timeout: Some(self.timeout),
            // One-shot: this process does not keep a reconnect loop alive, so a node that goes
            // away is a per-job refusal rather than a background thread quietly retrying.
            strategy: ConnectStrategy::Fallback,
            ..Default::default()
        };
        client.connect(Some(options)).await.map_err(|e| e.to_string())?;
        let dag = client.get_block_dag_info().await.map_err(|e| e.to_string())?;
        let producer = client
            .get_palw_producer_facts(self.class_id.clone(), self.bond_txid.clone(), self.bond_index, !self.bond_txid.is_empty())
            .await
            .map_err(|e| e.to_string())?;
        let _ = client.disconnect().await;
        Ok(((dag.sink.to_string(), dag.virtual_daa_score), producer))
    }
}

/// `host:port` becomes the borsh wRPC URL the CLI's `--rpc` means; anything already carrying a
/// scheme is left alone.
pub fn normalize_endpoint(endpoint: &str) -> String {
    if endpoint.contains("://") { endpoint.to_string() } else { format!("ws://{endpoint}") }
}

/// The `u128` decimal strings the producer-facts surface uses (ADR-0046: derive, never declare).
/// An unparseable number is ZERO, not a panic and not a large default: this value gates a spend,
/// and the safe reading of "I could not read the ceiling" is "there is no room".
fn decimal_u128(s: &str) -> u128 {
    s.parse::<u128>().unwrap_or(0)
}

/// `ceiling − reserved`, saturating into `u64` sompi. Both are `u128` on the wire because exposure
/// is priced in `pwu × slash_value_per_pwu`, and the difference is what one more claim may spend.
pub fn exposure_room(ceiling: &str, reserved: &str) -> u64 {
    decimal_u128(ceiling).saturating_sub(decimal_u128(reserved)).min(u64::MAX as u128) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn certified() -> ChainFacts {
        ChainFacts {
            source: "rpc ws://127.0.0.1:17610".into(),
            live: true,
            registered: true,
            fp_certified: true,
            bond_active: true,
            exposure_room_sompi: 1_000_000,
            claim_exposure_sompi: 50_000,
            fp_quanta_per_canonical_job: 8,
            fp_max_quanta_per_receipt: 64,
            anchor_daa: 100,
            ..Default::default()
        }
    }

    /// **Decision 3's four names, each one refusing by itself.** The order matters only in that
    /// every refusal is a sentence an operator can act on, and none of them is "false".
    #[test]
    fn each_chain_side_reason_refuses_the_commitment_by_name() {
        assert!(certified().commit_refusal().is_none(), "a certified class with room has no chain-side objection");

        let cases: Vec<(ChainFacts, &str)> = vec![
            (ChainFacts { registered: false, ..certified() }, "does not know this class"),
            (ChainFacts { fp_certified: false, ..certified() }, "free-prompt lane"),
            (ChainFacts { fp_quanta_per_canonical_job: 0, ..certified() }, "prices no free-prompt lane"),
            (ChainFacts { bond_active: false, ..certified() }, "does not know this bond"),
            (
                ChainFacts { bond_active: false, bond_not_ready_reason: "the bond is Retiring".into(), ..certified() },
                "the bond is Retiring",
            ),
            (ChainFacts { exposure_room_sompi: 0, ..certified() }, "no room for another claim"),
            (ChainFacts { read_error: Some("connection refused".into()), ..certified() }, "could not be read"),
        ];
        for (facts, needle) in cases {
            let refusal = facts.commit_refusal().unwrap_or_else(|| panic!("expected a refusal naming {needle:?}"));
            assert!(refusal.contains(needle), "the refusal must name {needle:?}, got: {refusal}");
        }
    }

    /// The offline form objects to nothing and claims nothing: four unknowns, and a `/health` that
    /// SAYS unknown rather than dropping the field. A field that disappears reads as fine.
    #[test]
    fn the_anchor_file_form_reports_unknowns_and_never_a_yes() {
        let offline = ChainFacts { source: "anchor file /tmp/anchor.json".into(), live: false, anchor_daa: 42, ..Default::default() };
        assert!(offline.commit_refusal().is_none(), "a source that cannot submit is not refused for facts it never had");
        let health = offline.health_json();
        for name in ["registered", "fp_certified", "bond_active", "exposure_room"] {
            assert_eq!(health[name], serde_json::json!("unknown"), "{name} must be present and honest");
        }
        // A node that could not be REACHED is the same unknown, and this is the trap: a failed
        // read leaves every gate at its `false` default, which reads exactly like a chain that
        // answered no. An operator would go looking for a certification they already have.
        let unreachable = ChainFacts { read_error: Some("connection refused".into()), ..certified() };
        let health = unreachable.health_json();
        for name in ["registered", "fp_certified", "bond_active", "exposure_room"] {
            assert_eq!(health[name], serde_json::json!("unknown"), "{name} is unknown when the node could not be read");
        }
        assert!(unreachable.commit_refusal().unwrap().contains("could not be read"), "and the refusal says which");

        // And the live form answers all four with real values, which is what Decision 3 asks for.
        let health = certified().health_json();
        for name in ["registered", "fp_certified", "bond_active"] {
            assert_eq!(health[name], serde_json::json!(true), "{name}");
        }
        assert_eq!(health["exposure_room"], serde_json::json!(1_000_000u64));
    }

    /// Exposure room is a subtraction on `u128` decimal strings, and an unreadable number is ZERO
    /// room — the safe reading of an unknown that gates a spend.
    #[test]
    fn exposure_room_is_the_difference_and_an_unreadable_number_is_no_room() {
        assert_eq!(exposure_room("10000", "2500"), 7_500);
        assert_eq!(exposure_room("2500", "10000"), 0, "a reserved past the ceiling is no room, never a wrap");
        assert_eq!(exposure_room("", "0"), 0);
        assert_eq!(exposure_room("nonsense", "0"), 0);
        // u128 quantities clamp into sompi rather than wrapping.
        assert_eq!(exposure_room(&u128::MAX.to_string(), "0"), u64::MAX);
    }

    #[test]
    fn an_endpoint_without_a_scheme_becomes_the_borsh_wrpc_url_the_cli_means() {
        assert_eq!(normalize_endpoint("127.0.0.1:17610"), "ws://127.0.0.1:17610");
        assert_eq!(normalize_endpoint("ws://node.example:17610"), "ws://node.example:17610");
        assert_eq!(normalize_endpoint("wss://node.example:17610"), "wss://node.example:17610");
    }
}
