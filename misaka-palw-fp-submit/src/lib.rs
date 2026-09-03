//! **ADR-0077 Decision 4: one handoff — and therefore one submit path.**
//!
//! ```text
//!   the gateway (per job, automatic)          `misaka palw fp-submit` (the manual form)
//!                     \                                   /
//!                      \                                 /
//!                       ────────  THIS CRATE  ───────────
//!                                     │
//!            the anchor is still fresh (SA-1b)      ─┐
//!            funding the mempool agrees is unspent   │
//!            the subnetwork check                    ├─ then: submit_transaction
//!            <claim>.material staged as .partial    ─┘   then: rename to its real name
//! ```
//!
//! Decision 4 says the gateway "calls the library the CLI's `fp-submit` calls; it does not shell
//! out to it". A second submission path would be a second place for *is this still fresh, which
//! funding, which subnetwork, when does the material become real* to be answered differently —
//! and each of those has already been answered wrongly once in this tree:
//!
//! * **Freshness.** ADR-0077 SA-1(b): a queued commitment expires WITH ITS ANCHOR and is never
//!   submitted stale. The gateway retires an outbox artifact past its TTL by renaming it
//!   `…​.expired`; that only closes half the loop, because a rail pointed at the stem it already
//!   read would still hold the bytes. The check therefore lives HERE, on the path that spends the
//!   fee, and it is made against the node's own `virtual_daa_score` — not against whatever DAA
//!   the caller last happened to read.
//! * **Funding.** `submit_objects` in the CLI records it: an earlier burst's carriers spend a UTXO
//!   the utxoindex still lists and pay change the index does not list yet, so picking by the index
//!   alone is "output already spent by transaction in the mempool", every second run. The mempool
//!   has to be asked, our own pending spends excluded and our own pending change made eligible.
//! * **Locked collateral.** A producer's PALW bond sits at its own pay address and is usually the
//!   largest output there, and selection is largest-first (audit3 H3/H12). The node publishes the
//!   whole must-not-spend set — consensus-locked collateral AND the outpoints its own panel has
//!   reserved — through `get_palw_producer_facts` with an empty class id, and a selector that does
//!   not read it spends the bond that backs the very claim it is submitting.
//! * **When the obligation becomes real.** The material is encoded and staged to a `.partial`
//!   BEFORE the broadcast and renamed only after acceptance. Staging it after used to surface a
//!   decode error only once the claim was already on chain, where re-running could not repair it:
//!   the second run hits `submit_transaction` first, which refuses an already-accepted transaction
//!   and returns before the material block. The claim then sits there certifiable by nobody, with
//!   its producer's bond carrying the exposure.
//!
//! **The shape that makes the ordering testable.** Everything that can be decided from local
//! bytes is decided in [`plan_submission`], which is a pure function; the ordering itself is
//! [`execute_handoff`], which takes the broadcast as a [`FpBroadcast`] rather than an `RpcApi`.
//! `submit_fp_commitment` is the thin wrapper that gives it a node. A test can therefore drive the
//! SHIPPED ordering — stage, broadcast, rename — with a broadcaster that fails on demand, without
//! a chain, a model or a key.
//!
//! **What this crate does NOT do.** It does not build or sign the commitment transaction — that is
//! `ValidatorKey::build_fp_commitment_tx`, whose two key forms (a local seed for devnets, the
//! `kaspa-pq-signer` sidecar in production) are the rail's and are unchanged. It takes a
//! transaction that is already signed and answers only the questions above.

use std::collections::{BTreeSet, HashSet};
use std::future::Future;
use std::path::{Path, PathBuf};

use kaspa_addresses::Address;
use kaspa_consensus_core::palw_freeprompt_v3::{
    PALW_FP_CAPTURE_V1_MAGIC, PALW_FP_MATERIAL_V1_MAGIC, PalwFpCommitmentTxPayloadV3, fp_claim_id_v3, palw_fp_capture_encode_v1,
    palw_fp_material_encode_v1,
};
use kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_FP_COMMITMENT;
use kaspa_consensus_core::tx::{Transaction, TransactionOutpoint, UtxoEntry};
use kaspa_hashes::Hash64;
use kaspa_rpc_core::{RpcTransaction, api::rpc::RpcApi};

/// The suffix the gateway renames a lapsed outbox commitment to (ADR-0077 SA-1b). Spelled here
/// because both ends of the loop must agree on it: the gateway writes it, and
/// [`load_unsigned_commitment`] refuses a stem that carries it.
pub const EXPIRED_SUFFIX: &str = ".expired";

/// Every way this path refuses, **by name**. A caller prints the variant; nothing here degrades to
/// a silent default, and nothing that could be checked before a fee is spent is checked after.
#[derive(Debug)]
pub enum FpSubmitError {
    /// The transaction is not a free-prompt commitment. Refused rather than handed to the node:
    /// this path exists for one kind of transaction, and submitting another under its name would
    /// put a spend on chain that the operator asked for by accident.
    WrongSubnetwork {
        got: String,
    },
    /// The transaction's payload does not decode as `PalwFpCommitmentTxPayloadV3`, so no material
    /// can be written for it and nothing should be broadcast.
    UndecodablePayload(String),
    /// The bytes handed over as the worker's capture are a `FPM1`/`FPC1` payload (or empty) — the
    /// one mistake a hand can make, pointing this at a staged material instead of at the run's own
    /// `material.bin`.
    NotACapture,
    /// The claim the DSL names is not the claim this transaction commits.
    DslClaimMismatch {
        dsl_claim: Hash64,
        tx_claim: Hash64,
    },
    /// **ADR-0077 SA-1(b).** The commitment's anchor is older than its TTL at the chain's current
    /// DAA: the freshness binding this job was drawn under has lapsed, and a claim submitted now
    /// would be one nobody promised. Nothing is staged, nothing is broadcast, no fee is spent.
    AnchorExpired {
        anchor_daa: u64,
        expires_at_daa: u64,
        chain_daa: u64,
    },
    /// The artifact this stem names was already retired by the gateway's sweep (`…​.expired`).
    /// Refused rather than read: the rename IS the decision, and a rail that reads through it
    /// re-opens exactly the window SA-1(b) closes.
    ArtifactRetired {
        path: PathBuf,
    },
    Io {
        what: String,
        error: String,
    },
    /// The node refused the transaction. `already_spent` is set when the reason was a funding
    /// UTXO an earlier submission's carrier still holds in the mempool — a wait, not a fault.
    Rejected {
        txid: String,
        error: String,
        already_spent: bool,
    },
    /// The node could not be asked something this path must know before it spends.
    Rpc {
        call: &'static str,
        error: String,
    },
    /// No funding at the address survives the three filters (mature, unlocked, unspent by our own
    /// mempool traffic) at the amount asked for.
    NoFunding {
        address: String,
        need: u64,
    },
}

impl std::fmt::Display for FpSubmitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WrongSubnetwork { got } => {
                write!(f, "this transaction carries subnetwork {got} — the free-prompt submit path takes 0x4a commitments only")
            }
            Self::UndecodablePayload(e) => write!(f, "this payload does not decode, so no material can be written: {e}"),
            Self::NotACapture => write!(f, "these bytes are not a family capture (expected the worker's material.bin)"),
            Self::DslClaimMismatch { dsl_claim, tx_claim } => {
                write!(f, "the DSL payload names claim {dsl_claim} but this transaction commits claim {tx_claim}")
            }
            Self::AnchorExpired { anchor_daa, expires_at_daa, chain_daa } => write!(
                f,
                "this commitment's anchor is DAA {anchor_daa} and it expired at {expires_at_daa}; the chain is at {chain_daa} \
                 (ADR-0077 SA-1b: a queued commitment expires with its anchor and is never submitted stale) — re-run the job"
            ),
            Self::ArtifactRetired { path } => write!(
                f,
                "{} was retired by the gateway's anchor sweep; it is evidence of work, not a submittable commitment",
                path.display()
            ),
            Self::Io { what, error } => write!(f, "{what}: {error}"),
            Self::Rejected { txid, error, already_spent } => {
                if *already_spent {
                    write!(
                        f,
                        "submit {txid}: the funding UTXO is spent by a transaction still in the mempool (an earlier submission's \
                         change has not been mined yet) — wait for a block and re-run; nothing was carried: {error}"
                    )
                } else {
                    write!(f, "submit {txid}: {error}")
                }
            }
            Self::Rpc { call, error } => write!(f, "{call}: {error}"),
            Self::NoFunding { address, need } => {
                write!(f, "no mature, unlocked, unspent UTXO at {address} holds more than the {need} sompi this submission needs")
            }
        }
    }
}

impl std::error::Error for FpSubmitError {}

fn io(what: impl Into<String>) -> impl FnOnce(std::io::Error) -> FpSubmitError {
    let what = what.into();
    move |e| FpSubmitError::Io { what, error: e.to_string() }
}

// -------------------------------------------------------------------------------------------
// SA-1(b): a commitment expires with its anchor
// -------------------------------------------------------------------------------------------

/// **How long past its anchor a queued commitment may still be submitted** (ADR-0077 SA-1b).
///
/// The anchor is the freshness binding a job was drawn under; past the TTL the job is work the
/// operator did, not a claim the chain should be asked to carry. The TTL is the caller's — the
/// gateway's `COMMITMENT_ANCHOR_TTL_DAA` — because it is a property of the network's cadence, not
/// of this code path; what is NOT the caller's is whether to check it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnchorExpiry {
    pub anchor_daa: u64,
    pub ttl_daa: u64,
}

impl AnchorExpiry {
    pub fn new(anchor_daa: u64, ttl_daa: u64) -> Self {
        Self { anchor_daa, ttl_daa }
    }

    /// The last DAA at which this commitment may still be submitted.
    pub fn expires_at(&self) -> u64 {
        self.anchor_daa.saturating_add(self.ttl_daa)
    }

    /// Strictly past the deadline is expired; AT the deadline is still fresh, so the gateway's own
    /// sweep (`current > anchor + ttl`) and this check retire exactly the same artifacts. Two
    /// different comparisons would be a window in which one end submits what the other retired.
    pub fn is_expired_at(&self, chain_daa: u64) -> bool {
        chain_daa > self.expires_at()
    }

    fn check(&self, chain_daa: u64) -> Result<(), FpSubmitError> {
        if self.is_expired_at(chain_daa) {
            return Err(FpSubmitError::AnchorExpired { anchor_daa: self.anchor_daa, expires_at_daa: self.expires_at(), chain_daa });
        }
        Ok(())
    }
}

/// **Read an unsigned commitment the gateway queued, refusing one its sweep already retired.**
///
/// The other half of SA-1(b)'s loop. The gateway renames a lapsed artifact rather than deleting it
/// (the artifact is evidence of work the operator did), and a rail that ignored the rename would
/// submit exactly the stale commitment the rename exists to stop. `path` is the artifact's real
/// name; the `…​.expired` sibling is the veto.
pub fn load_unsigned_commitment(path: &Path) -> Result<Vec<u8>, FpSubmitError> {
    // Naming the retired file directly is not a way around the rename — the suffix IS the verdict,
    // whichever of the two names the caller reaches for.
    if path.file_name().and_then(|n| n.to_str()).is_some_and(|name| name.ends_with(EXPIRED_SUFFIX)) {
        return Err(FpSubmitError::ArtifactRetired { path: path.to_path_buf() });
    }
    let retired = PathBuf::from(format!("{}{EXPIRED_SUFFIX}", path.display()));
    if retired.exists() {
        return Err(FpSubmitError::ArtifactRetired { path: retired });
    }
    std::fs::read(path).map_err(io(format!("cannot read the queued commitment at {}", path.display())))
}

// -------------------------------------------------------------------------------------------
// The material: what a seat is served when it asks this claim's producer for the job
// -------------------------------------------------------------------------------------------

/// Decode the commitment payload a signed 0x4a transaction carries, and name the claim it opens.
///
/// Done before anything is broadcast: a payload that does not decode is a transaction whose
/// material can never be written, and that is a fact about the file on disk, not about the chain.
pub fn decode_commitment_payload(tx: &Transaction) -> Result<(PalwFpCommitmentTxPayloadV3, Hash64), FpSubmitError> {
    if tx.subnetwork_id != SUBNETWORK_ID_PALW_FP_COMMITMENT {
        return Err(FpSubmitError::WrongSubnetwork { got: tx.subnetwork_id.to_string() });
    }
    let payload: PalwFpCommitmentTxPayloadV3 =
        borsh::from_slice(&tx.payload).map_err(|e| FpSubmitError::UndecodablePayload(e.to_string()))?;
    let claim = fp_claim_id_v3(&payload.commitment);
    Ok((payload, claim))
}

/// **`FPC1` with the capture, `FPM1` without** (ADR-0073 Decision 1a).
///
/// With the capture, the answer travels beside the question and a seat can verify without
/// re-running; without it, the question alone, and a seat's only verifier is a re-run. The
/// capture is checked here rather than discovered by a seat: the one hand-mistake this catches is
/// a caller pointing at an already-staged material instead of at the run's own `material.bin`,
/// which encodes cleanly and verifies as nothing.
pub fn encode_claim_material(payload: &PalwFpCommitmentTxPayloadV3, capture: Option<&[u8]>) -> Result<Vec<u8>, FpSubmitError> {
    match capture {
        Some(bytes) => {
            check_capture_shape(bytes)?;
            Ok(palw_fp_capture_encode_v1(&payload.commitment.job, &payload.prompt_token_ids, bytes))
        }
        None => Ok(palw_fp_material_encode_v1(&payload.commitment.job, &payload.prompt_token_ids)),
    }
}

/// The capture-shape check on its own, so a caller can refuse a bad path before it has a payload
/// to encode against (the CLI reads the capture from a file the operator names).
pub fn check_capture_shape(bytes: &[u8]) -> Result<(), FpSubmitError> {
    if bytes.is_empty() || bytes.starts_with(&PALW_FP_MATERIAL_V1_MAGIC) || bytes.starts_with(&PALW_FP_CAPTURE_V1_MAGIC) {
        return Err(FpSubmitError::NotACapture);
    }
    Ok(())
}

/// A file written under its `.partial` name, waiting for the broadcast to make it real.
///
/// The rule this type exists to make unforgettable: a reader — the node's retention resolver, a
/// seat pulling the material — must never see a half-written obligation, and must never see a
/// whole one for a claim the chain refused.
#[derive(Debug)]
pub struct StagedFile {
    partial: PathBuf,
    committed: PathBuf,
}

impl StagedFile {
    /// Write `bytes` to `<dir>/<name>.partial`.
    pub fn stage(dir: &Path, name: &str, bytes: &[u8]) -> Result<Self, FpSubmitError> {
        std::fs::create_dir_all(dir).map_err(io(format!("cannot create {}", dir.display())))?;
        let committed = dir.join(name);
        let partial =
            committed.with_extension(format!("{}.partial", committed.extension().and_then(|e| e.to_str()).unwrap_or_default()));
        std::fs::write(&partial, bytes).map_err(io(format!("cannot write {}", partial.display())))?;
        Ok(Self { partial, committed })
    }

    /// The chain accepted the claim: the obligation is real, so the file takes its real name.
    pub fn commit(self) -> Result<PathBuf, FpSubmitError> {
        std::fs::rename(&self.partial, &self.committed).map_err(io(format!("cannot commit {}", self.committed.display())))?;
        Ok(self.committed)
    }

    /// The chain never saw the claim: a material for a claim that does not exist would make a
    /// panel replay a ghost, so the partial goes away.
    pub fn discard(self) {
        let _ = std::fs::remove_file(&self.partial);
    }

    pub fn committed_path(&self) -> &Path {
        &self.committed
    }
}

/// What to stage into the node's retention directory alongside the broadcast.
#[derive(Default)]
pub struct FpStaging<'a> {
    /// The node's retention directory (`--palw-retention-dir`). `None` stages nothing, which is
    /// only ever right for a caller that has already retained the material itself.
    pub retention_dir: Option<&'a Path>,
    /// The worker's own capture (`material.bin`). `None` yields the question-only `FPM1` form.
    pub capture: Option<&'a [u8]>,
    /// ADR-0078 Decision 6: this claim's `FPD1` DSL payload, when the operator elected the DSL
    /// into the data-availability obligation. Off by default, and "off" means no file for the
    /// node to serve.
    pub dsl_payload: Option<&'a [u8]>,
    /// ADR-0077 SA-1(b): the anchor freshness this commitment was drawn under. `None` is the
    /// operator saying "I have no anchor policy here", which is only ever right for the manual
    /// form re-submitting a claim it just built.
    pub expiry: Option<AnchorExpiry>,
}

/// **Everything decided from local bytes, decided before anything is written or spent.**
///
/// Produced by [`plan_submission`] and consumed by [`execute_handoff`]. Holding the two apart is
/// what makes the ordering a thing a test can drive: a plan is pure, and an execution is the
/// three side effects in the one order they may happen in.
#[derive(Debug)]
pub struct SubmissionPlan {
    pub claim_id: Hash64,
    pub txid: String,
    /// `(file name, bytes)` — `None` when the caller retains the material itself.
    pub material: Option<(String, Vec<u8>)>,
    pub dsl: Option<(String, Vec<u8>)>,
    /// The DAA past which this commitment may not be submitted, when the caller declared an
    /// anchor policy. Echoed so a caller can log the bound it just cleared.
    pub expires_at_daa: Option<u64>,
}

/// **Decide everything a local machine can decide** — the subnetwork, the payload, the freshness,
/// the material encoding, the DSL's claim binding. Pure: no file is written, no fee is spent, and
/// every error here is a fact about the bytes rather than about the chain.
pub fn plan_submission(tx: &Transaction, staging: &FpStaging<'_>, chain_daa: u64) -> Result<SubmissionPlan, FpSubmitError> {
    let (payload, claim_id) = decode_commitment_payload(tx)?;
    // SA-1(b) FIRST: an expired commitment must not even cause a `.partial` to appear, because a
    // partial that is never renamed is a file an operator will find and wonder about.
    if let Some(expiry) = staging.expiry {
        expiry.check(chain_daa)?;
    }
    let (material, dsl) = match staging.retention_dir {
        Some(_) => {
            let bytes = encode_claim_material(&payload, staging.capture)?;
            let dsl = match staging.dsl_payload {
                Some(dsl_bytes) => {
                    let decoded = kaspa_consensus_core::palw_derived_v1::palw_fp_dsl_decode_v1(dsl_bytes)
                        .ok_or_else(|| FpSubmitError::UndecodablePayload("not an FPD1 DSL payload".into()))?;
                    if decoded.claim_id != claim_id {
                        return Err(FpSubmitError::DslClaimMismatch { dsl_claim: decoded.claim_id, tx_claim: claim_id });
                    }
                    Some((format!("{claim_id}.dsl"), dsl_bytes.to_vec()))
                }
                None => None,
            };
            (Some((format!("{claim_id}.material"), bytes)), dsl)
        }
        None => {
            if staging.dsl_payload.is_some() {
                return Err(FpSubmitError::Io {
                    what: "a DSL payload was offered with no retention directory to serve it from".into(),
                    error: "set retention_dir".into(),
                });
            }
            (None, None)
        }
    };
    Ok(SubmissionPlan { claim_id, txid: tx.id().to_string(), material, dsl, expires_at_daa: staging.expiry.map(|e| e.expires_at()) })
}

/// What a completed submission produced.
#[derive(Debug)]
pub struct FpSubmitted {
    pub txid: String,
    pub claim_id: Hash64,
    pub material_path: Option<PathBuf>,
    pub dsl_path: Option<PathBuf>,
}

/// The broadcast, as a capability rather than a client. `submit_fp_commitment` supplies the node;
/// a test supplies a closure that accepts or refuses on demand, and drives the SAME ordering.
pub trait FpBroadcast {
    fn broadcast(&self, tx: &Transaction) -> impl Future<Output = Result<String, String>>;
}

/// The node, as a broadcaster. A newtype rather than a blanket impl on `&dyn RpcApi`, because
/// `dyn RpcApi` does not implement `RpcApi` and a blanket impl would silently not apply to the one
/// caller that matters.
pub struct NodeBroadcast<'a>(pub &'a dyn RpcApi);

impl FpBroadcast for NodeBroadcast<'_> {
    async fn broadcast(&self, tx: &Transaction) -> Result<String, String> {
        self.0.submit_transaction(RpcTransaction::from(tx), false).await.map(|id| id.to_string()).map_err(|e| e.to_string())
    }
}

/// **The order, and nothing else.** Stage every file under `.partial`; broadcast; rename on
/// acceptance, remove on refusal. A failure at each step leaves exactly one consistent world — no
/// file and no claim; a `.partial` and no claim; the claim and its material.
pub async fn execute_handoff(
    plan: &SubmissionPlan,
    tx: &Transaction,
    retention_dir: Option<&Path>,
    broadcaster: impl FpBroadcast,
) -> Result<FpSubmitted, FpSubmitError> {
    let mut staged_material = None;
    let mut staged_dsl = None;
    if let Some(dir) = retention_dir {
        if let Some((name, bytes)) = &plan.material {
            staged_material = Some(StagedFile::stage(dir, name, bytes)?);
        }
        if let Some((name, bytes)) = &plan.dsl {
            match StagedFile::stage(dir, name, bytes) {
                Ok(file) => staged_dsl = Some(file),
                Err(e) => {
                    // Half a staging is not a staging: the material for a claim whose DSL could
                    // not be written would advertise an obligation this producer cannot serve.
                    if let Some(material) = staged_material {
                        material.discard();
                    }
                    return Err(e);
                }
            }
        }
    }

    let submitted = match broadcaster.broadcast(tx).await {
        Ok(id) => id,
        Err(text) => {
            // The staged files are not a claim's material if the claim never reached the chain.
            if let Some(material) = staged_material {
                material.discard();
            }
            if let Some(dsl) = staged_dsl {
                dsl.discard();
            }
            let already_spent = text.contains("already spent");
            return Err(FpSubmitError::Rejected { txid: plan.txid.clone(), error: text, already_spent });
        }
    };

    let material_path = match staged_material {
        Some(material) => Some(material.commit()?),
        None => None,
    };
    let dsl_path = match staged_dsl {
        Some(dsl) => Some(dsl.commit()?),
        None => None,
    };
    Ok(FpSubmitted { txid: submitted, claim_id: plan.claim_id, material_path, dsl_path })
}

/// The chain's own DAA, for the freshness check. Read from the node rather than taken from the
/// caller: a caller's idea of "now" is exactly the thing SA-1(b) says goes stale.
pub async fn read_chain_daa(client: &dyn RpcApi) -> Result<u64, FpSubmitError> {
    client
        .get_block_dag_info()
        .await
        .map(|info| info.virtual_daa_score)
        .map_err(|e| FpSubmitError::Rpc { call: "getBlockDagInfo", error: e.to_string() })
}

/// **Submit one free-prompt commitment and stage its material in the same step.**
///
/// Plan (pure), then execute (the ordering). The node is asked for its own DAA first, so the
/// SA-1(b) freshness check is made against the chain and not against the caller's last read.
pub async fn submit_fp_commitment(
    client: &dyn RpcApi,
    tx: &Transaction,
    staging: FpStaging<'_>,
) -> Result<FpSubmitted, FpSubmitError> {
    let chain_daa = match staging.expiry {
        Some(_) => read_chain_daa(client).await?,
        // No anchor policy declared: nothing reads this, and asking the node for a number the
        // caller said it does not use would be a round trip that changes no decision.
        None => 0,
    };
    let plan = plan_submission(tx, &staging, chain_daa)?;
    execute_handoff(&plan, tx, staging.retention_dir, NodeBroadcast(client)).await
}

// -------------------------------------------------------------------------------------------
// Funding: the three filters, in one place
// -------------------------------------------------------------------------------------------

/// The maturity facts a funding decision needs, read from the node once per selection.
///
/// Both coinbase gates, because a wallet that checks only the maturity floor offers money the node
/// refuses (ADR-0018): on testnet-11 the floor is 1 and settlement is 600, so every coinbase
/// younger than 600 DAA read as spendable and every send of one was rejected.
#[derive(Clone, Copy, Debug)]
pub struct FpFundingPolicy {
    pub virtual_daa: u64,
    pub coinbase_maturity: u64,
    /// `0` is the settlement feature being off, and then this is the floor alone.
    pub settlement_long_maturity_daa: u64,
}

/// One spendable output, as the selector found it.
#[derive(Clone, Debug)]
pub struct FpFunding {
    pub outpoint: TransactionOutpoint,
    pub entry: UtxoEntry,
}

/// **Every outpoint at this address that must not be spent, as the NODE reports it.**
///
/// Two sources in one list (audit3 H3/H12): PALW bond collateral consensus still locks, and the
/// outpoints this node's own panel has reserved to fund the lifecycle objects it carries. A
/// gateway that spends the first destroys the backing of the very claim it is submitting; one that
/// spends the second wedges its node's panel. Asked with an EMPTY class id, which is the request
/// shape that means "I have no class to name, give me the set".
pub async fn locked_outpoints(client: &dyn RpcApi) -> Result<HashSet<TransactionOutpoint>, FpSubmitError> {
    let facts = client
        .get_palw_producer_facts(String::new(), String::new(), 0, false)
        .await
        .map_err(|e| FpSubmitError::Rpc { call: "getPalwProducerFacts", error: e.to_string() })?;
    parse_locked_outpoints(&facts.locked_bond_outpoints)
}

/// The parse, separated so the "a short list is worse than no list" rule is testable: an entry
/// this cannot read is an ERROR, never a skip. The set is a safety list, and silently shortening
/// it is the exact failure it exists to prevent.
pub fn parse_locked_outpoints(entries: &[String]) -> Result<HashSet<TransactionOutpoint>, FpSubmitError> {
    let mut out = HashSet::new();
    for entry in entries {
        let bad = |what: &str| FpSubmitError::Rpc { call: "getPalwProducerFacts", error: format!("locked outpoint {entry:?} {what}") };
        let Some((txid, index)) = entry.rsplit_once(':') else {
            return Err(bad("has no index"));
        };
        let transaction_id = txid.parse::<kaspa_consensus_core::tx::TransactionId>().map_err(|_| bad("is not a txid"))?;
        let index: u32 = index.parse().map_err(|_| bad("has no index"))?;
        out.insert(TransactionOutpoint::new(transaction_id, index));
    }
    Ok(out)
}

/// **Ask the mempool before choosing funding.**
///
/// Returns (what our own pending transactions have already spent, what change they will pay us).
/// The utxoindex lists the first and does not list the second, so a selector reading the index
/// alone re-picks a spent outpoint on every second run — the failure `submit_objects` records as
/// "output already spent by transaction in the mempool".
async fn mempool_view(
    client: &dyn RpcApi,
    address: &Address,
) -> Result<(HashSet<TransactionOutpoint>, Vec<FpFunding>), FpSubmitError> {
    use kaspa_consensus_core::tx::{TransactionInput, TransactionOutput};
    let mut spent = HashSet::new();
    let mut change = Vec::new();
    let entries = match client.get_mempool_entries_by_addresses(vec![address.clone()], false, false).await {
        Ok(entries) => entries,
        // A node without a mempool index answers nothing rather than lying; the caller's other two
        // filters still apply and the worst case is the retry this function exists to avoid.
        Err(_) => return Ok((spent, change)),
    };
    let my_spk = kaspa_txscript::pay_to_address_script(address);
    let mut seen: BTreeSet<kaspa_consensus_core::tx::TransactionId> = Default::default();
    for by_address in entries {
        for entry in by_address.sending.iter().chain(by_address.receiving.iter()) {
            let rtx = &entry.transaction;
            let inputs: Vec<TransactionInput> = rtx
                .inputs
                .iter()
                .map(|i| {
                    TransactionInput::new(
                        TransactionOutpoint::new(i.previous_outpoint.transaction_id, i.previous_outpoint.index),
                        i.signature_script.clone(),
                        i.sequence,
                        i.sig_op_count,
                    )
                })
                .collect();
            let outputs: Vec<TransactionOutput> =
                rtx.outputs.iter().map(|o| TransactionOutput::new(o.value, o.script_public_key.clone())).collect();
            let tx =
                Transaction::new(rtx.version, inputs, outputs, rtx.lock_time, rtx.subnetwork_id.clone(), rtx.gas, rtx.payload.clone());
            let id = tx.id();
            if !seen.insert(id) {
                continue;
            }
            for input in &tx.inputs {
                spent.insert(input.previous_outpoint);
            }
            for (index, output) in tx.outputs.iter().enumerate() {
                if output.script_public_key == my_spk {
                    change.push(FpFunding {
                        outpoint: TransactionOutpoint::new(id, index as u32),
                        entry: UtxoEntry::new(output.value, output.script_public_key.clone(), 0, false),
                    });
                }
            }
        }
    }
    Ok((spent, change))
}

/// **The one funding selector this lane uses**, largest first, after three filters that each close
/// a failure this tree has actually had: mature (both coinbase gates), not locked collateral or a
/// panel reservation, and not already spent by our own mempool traffic. Our own pending change is
/// eligible, so a burst of submissions chains rather than colliding.
///
/// `must_exceed` is a strict bound: the difference pays the change output, and a zero-value output
/// is dust the node refuses.
pub async fn select_funding(
    client: &dyn RpcApi,
    address: &Address,
    must_exceed: u64,
    policy: FpFundingPolicy,
) -> Result<FpFunding, FpSubmitError> {
    let locked = locked_outpoints(client).await?;
    let (pending_spent, pending_change) = mempool_view(client, address).await?;

    let mut candidates: Vec<FpFunding> = Vec::new();
    let mut cursor = String::new();
    loop {
        let page = client
            .get_utxos_by_address_page(address.clone(), cursor.clone(), 1000)
            .await
            .map_err(|e| FpSubmitError::Rpc { call: "getUtxosByAddressPage", error: e.to_string() })?;
        for e in page.entries {
            let outpoint: TransactionOutpoint = e.outpoint.into();
            if locked.contains(&outpoint) || pending_spent.contains(&outpoint) {
                continue;
            }
            if !kaspa_pq_validator_core::is_spendable_settled(
                e.utxo_entry.is_coinbase,
                e.utxo_entry.block_daa_score,
                policy.virtual_daa,
                policy.coinbase_maturity,
                policy.settlement_long_maturity_daa,
                None,
            ) {
                continue;
            }
            candidates.push(FpFunding { outpoint, entry: e.utxo_entry.into() });
        }
        if page.next_cursor.is_empty() {
            break;
        }
        cursor = page.next_cursor;
    }
    candidates.extend(pending_change.into_iter().filter(|c| !pending_spent.contains(&c.outpoint) && !locked.contains(&c.outpoint)));
    candidates.sort_by(|a, b| b.entry.amount.cmp(&a.entry.amount));
    candidates
        .into_iter()
        .find(|c| c.entry.amount > must_exceed)
        .ok_or_else(|| FpSubmitError::NoFunding { address: address.to_string(), need: must_exceed })
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::subnets::SubnetworkId;
    use kaspa_consensus_core::tx::{ScriptPublicKey, TransactionInput, TransactionOutput};

    /// A signed 0x4a transaction is not something this crate can build (that is the rail's key),
    /// so the tests build the ONE thing this crate reads: a transaction on the right subnetwork
    /// whose payload is a real `PalwFpCommitmentTxPayloadV3`.
    fn commitment_tx(payload_bytes: Vec<u8>, subnetwork: SubnetworkId) -> Transaction {
        Transaction::new(
            0,
            vec![TransactionInput::new(TransactionOutpoint::new(Default::default(), 0), vec![], 0, 1)],
            vec![TransactionOutput::new(1, ScriptPublicKey::default())],
            0,
            subnetwork,
            0,
            payload_bytes,
        )
    }

    /// The commitment a plan reads. Built by hand rather than signed: this crate never signs, and
    /// what it reads is the payload's shape, not its witness.
    fn sample_payload() -> (PalwFpCommitmentTxPayloadV3, Vec<u8>) {
        use kaspa_consensus_core::palw_freeprompt_v3::{
            PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_PROMPT_MODE_USER, PALW_FP_V3_VERSION, PalwFpStopReasonV3, PalwFreePromptCommitmentV3,
            PalwFreePromptJobV3,
        };
        let ids: Vec<u32> = (1..=8u32).collect();
        let job = PalwFreePromptJobV3 {
            version: PALW_FP_V3_VERSION,
            network_domain: Hash64::from_u64_word(0x11),
            class_id: Hash64::from_u64_word(0x22),
            executor_bond: TransactionOutpoint::new(Default::default(), 0),
            executor_pubkey: vec![9u8; 32],
            operator_id: Hash64::from_u64_word(0x33),
            anchor_block: Hash64::from_u64_word(0x44),
            anchor_daa: 100_000,
            job_nonce: [5u8; 32],
            tokenizer_id: Hash64::from_u64_word(0x55),
            prompt_token_ids_hash: kaspa_consensus_core::palw_v2::prompt_token_ids_hash_v2(&ids),
            prompt_tokens: ids.len() as u32,
            decode_token_limit: 16,
            max_context_tokens: 512,
            privacy_mode: PALW_FP_PRIVACY_PUBLIC_DA,
            prompt_mode: PALW_FP_PROMPT_MODE_USER,
            sampling_seed: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_SEED_GREEDY,
            temperature_q: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_TEMPERATURE_GREEDY,
        };
        let commitment = PalwFreePromptCommitmentV3 {
            job,
            trace_root: Hash64::from_u64_word(0x66),
            output_root: Hash64::from_u64_word(0x77),
            schedule_root: Hash64::from_u64_word(0x88),
            execution_root: Hash64::from_u64_word(0x99),
            decode_tokens_executed: 16,
            stop_reason: PalwFpStopReasonV3::ExactBudgetReached,
            work_leaves: 4_096,
            trace_manifest_root: Hash64::from_u64_word(0xAA),
            trace_chunk_count: 1,
            trace_retention_daa: 200_000,
        };
        let payload =
            PalwFpCommitmentTxPayloadV3 { version: PALW_FP_V3_VERSION, commitment, prompt_token_ids: ids, signature: vec![7u8; 32] };
        let bytes = borsh::to_vec(&payload).unwrap();
        (payload, bytes)
    }

    struct Accept;
    impl FpBroadcast for Accept {
        async fn broadcast(&self, tx: &Transaction) -> Result<String, String> {
            Ok(tx.id().to_string())
        }
    }
    struct Refuse(&'static str);
    impl FpBroadcast for Refuse {
        async fn broadcast(&self, _tx: &Transaction) -> Result<String, String> {
            Err(self.0.to_string())
        }
    }

    fn tempdir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("fp-submit-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// The staging discipline, both endings. A reader must never see a whole file for a claim the
    /// chain refused, and must never see a half one at all.
    #[test]
    fn staging_is_partial_until_it_is_committed() {
        let dir = tempdir("stage");
        let staged = StagedFile::stage(&dir, "abc.material", b"body").unwrap();
        assert!(!staged.committed_path().exists(), "the real name must not exist before the broadcast is accepted");
        assert!(dir.join("abc.material.partial").exists(), "the partial is what is on disk while the claim is in flight");
        let path = staged.commit().unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"body");
        assert!(!dir.join("abc.material.partial").exists(), "the partial is gone once the file is real");

        let discarded = StagedFile::stage(&dir, "def.material", b"body").unwrap();
        discarded.discard();
        assert!(!dir.join("def.material").exists());
        assert!(!dir.join("def.material.partial").exists(), "a refused claim leaves nothing behind");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The one hand-mistake the capture check exists for: pointing this at a staged material
    /// instead of at the run's own `material.bin`. Both magics and the empty case are refused.
    #[test]
    fn a_staged_material_is_not_a_capture() {
        let mut fpm = PALW_FP_MATERIAL_V1_MAGIC.to_vec();
        fpm.extend_from_slice(b"whatever");
        let mut fpc = PALW_FP_CAPTURE_V1_MAGIC.to_vec();
        fpc.extend_from_slice(b"whatever");
        for bytes in [Vec::new(), fpm, fpc] {
            assert!(
                matches!(check_capture_shape(&bytes), Err(FpSubmitError::NotACapture)),
                "a payload must never be staged as if it were a capture"
            );
        }
        assert!(check_capture_shape(b"\x01\x02\x03 a family tuple").is_ok());
    }

    /// **Transaction assembly, as this path reads it.** The subnetwork is checked before the
    /// payload, the payload is decoded before anything is written, and the claim id the plan
    /// carries is the one `fp_claim_id_v3` derives from the commitment inside it.
    #[test]
    fn the_plan_reads_the_transaction_and_refuses_anything_else() {
        let (payload, bytes) = sample_payload();
        let expected_claim = fp_claim_id_v3(&payload.commitment);

        let tx = commitment_tx(bytes.clone(), SUBNETWORK_ID_PALW_FP_COMMITMENT);
        let plan = plan_submission(&tx, &FpStaging::default(), 0).unwrap();
        assert_eq!(plan.claim_id, expected_claim, "the claim a submission opens is the payload's, never the caller's");
        assert_eq!(plan.txid, tx.id().to_string());
        assert!(plan.material.is_none(), "no retention dir stages nothing");

        // A transaction on any other subnetwork is refused BEFORE its payload is looked at: this
        // path exists for one kind of transaction and submitting another under its name would put
        // a spend on chain the operator asked for by accident.
        let wrong = commitment_tx(bytes.clone(), kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE);
        assert!(matches!(plan_submission(&wrong, &FpStaging::default(), 0), Err(FpSubmitError::WrongSubnetwork { .. })));

        // A payload that does not decode is a transaction whose material can never be written.
        let junk = commitment_tx(b"not a commitment".to_vec(), SUBNETWORK_ID_PALW_FP_COMMITMENT);
        assert!(matches!(plan_submission(&junk, &FpStaging::default(), 0), Err(FpSubmitError::UndecodablePayload(_))));

        // With a retention dir the material is planned (not yet written) under the claim's name.
        let dir = tempdir("plan");
        let staging = FpStaging { retention_dir: Some(dir.as_path()), ..Default::default() };
        let plan = plan_submission(&tx, &staging, 0).unwrap();
        let (name, encoded) = plan.material.as_ref().expect("a retention dir stages the material");
        assert_eq!(name, &format!("{expected_claim}.material"));
        assert!(encoded.starts_with(&PALW_FP_MATERIAL_V1_MAGIC), "no capture is the question-only FPM1 form");
        assert!(!dir.join(name).exists(), "planning writes NOTHING");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **ADR-0077 SA-1(b): a queued commitment expires with its anchor.** At the deadline it is
    /// still fresh; one DAA past it nothing is staged, nothing is broadcast, and the error names
    /// all three numbers.
    #[test]
    fn a_stale_anchor_is_refused_before_anything_is_written() {
        let (payload, bytes) = sample_payload();
        let anchor = payload.commitment.job.anchor_daa;
        let tx = commitment_tx(bytes, SUBNETWORK_ID_PALW_FP_COMMITMENT);
        let dir = tempdir("expiry");
        let expiry = AnchorExpiry::new(anchor, 3_000);
        assert_eq!(expiry.expires_at(), anchor + 3_000);
        assert!(!expiry.is_expired_at(anchor + 3_000), "AT the deadline is still fresh — the same comparison the sweep uses");
        assert!(expiry.is_expired_at(anchor + 3_001));

        let staging = FpStaging { retention_dir: Some(dir.as_path()), expiry: Some(expiry), ..Default::default() };
        plan_submission(&tx, &staging, anchor + 3_000).expect("fresh at the deadline");

        let err = plan_submission(&tx, &staging, anchor + 3_001).unwrap_err();
        match err {
            FpSubmitError::AnchorExpired { anchor_daa, expires_at_daa, chain_daa } => {
                assert_eq!((anchor_daa, expires_at_daa, chain_daa), (anchor, anchor + 3_000, anchor + 3_001));
            }
            other => panic!("a stale anchor must be named, got {other}"),
        }
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0, "an expired commitment leaves not even a .partial");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **The other half of SA-1(b)'s loop.** The gateway's sweep renames a lapsed artifact; a rail
    /// that reads through the rename re-opens the window the rename closed.
    #[test]
    fn a_retired_artifact_is_never_read_back() {
        let dir = tempdir("retired");
        let live = dir.join("fp-job-abc.commitment-unsigned.borsh");
        std::fs::write(&live, b"bytes").unwrap();
        assert_eq!(load_unsigned_commitment(&live).unwrap(), b"bytes");

        let retired = PathBuf::from(format!("{}{EXPIRED_SUFFIX}", live.display()));
        std::fs::rename(&live, &retired).unwrap();
        assert!(matches!(load_unsigned_commitment(&live), Err(FpSubmitError::ArtifactRetired { .. })));
        assert!(
            matches!(load_unsigned_commitment(&retired), Err(FpSubmitError::ArtifactRetired { .. })),
            "naming the retired file directly is not a way around the rename"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **The handoff ordering, driven on the shipped path.** Accepted: the material takes its real
    /// name. Refused: nothing is left behind — not the real name and not the partial — and the
    /// mempool-collision reason is recognised so a caller can retry instead of re-running the model.
    #[test]
    fn the_handoff_stages_then_broadcasts_then_renames() {
        let (payload, bytes) = sample_payload();
        let claim = fp_claim_id_v3(&payload.commitment);
        let tx = commitment_tx(bytes, SUBNETWORK_ID_PALW_FP_COMMITMENT);
        let dir = tempdir("handoff");
        let staging = FpStaging { retention_dir: Some(dir.as_path()), ..Default::default() };
        let plan = plan_submission(&tx, &staging, 0).unwrap();

        let refused = futures::executor::block_on(execute_handoff(
            &plan,
            &tx,
            Some(dir.as_path()),
            Refuse("output already spent by transaction in the mempool"),
        ))
        .unwrap_err();
        match &refused {
            FpSubmitError::Rejected { already_spent, .. } => assert!(already_spent, "a mempool collision is a wait, not a fault"),
            other => panic!("expected a rejection, got {other}"),
        }
        assert_eq!(
            std::fs::read_dir(&dir).unwrap().count(),
            0,
            "a refused claim leaves no material and no partial — a seat must never replay a ghost"
        );

        let done = futures::executor::block_on(execute_handoff(&plan, &tx, Some(dir.as_path()), Accept)).unwrap();
        assert_eq!(done.claim_id, claim);
        assert_eq!(done.txid, tx.id().to_string());
        let path = done.material_path.expect("an accepted claim's material is real");
        assert_eq!(path, dir.join(format!("{claim}.material")));
        assert!(path.is_file());
        assert!(!dir.join(format!("{claim}.material.partial")).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A DSL elected into the DA obligation must name the claim it rides with, and a mismatch is
    /// caught in the PLAN — before a material file exists for a claim whose DSL is another's.
    #[test]
    fn a_dsl_for_another_claim_is_refused_in_the_plan() {
        let (_payload, bytes) = sample_payload();
        let tx = commitment_tx(bytes, SUBNETWORK_ID_PALW_FP_COMMITMENT);
        let dir = tempdir("dsl");
        let foreign = kaspa_consensus_core::palw_derived_v1::palw_fp_dsl_encode_v1(
            Hash64::from_u64_word(0xD5),
            Hash64::from_u64_word(0xD6),
            Hash64::from_u64_word(0xD7),
            b"{\"nodes\":[]}",
        );
        let staging = FpStaging { retention_dir: Some(dir.as_path()), dsl_payload: Some(&foreign), ..Default::default() };
        assert!(matches!(plan_submission(&tx, &staging, 0), Err(FpSubmitError::DslClaimMismatch { .. })));
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);

        // And a DSL with nowhere to be served from is a configuration error, not a silent drop.
        let nowhere = FpStaging { dsl_payload: Some(&foreign), ..Default::default() };
        assert!(matches!(plan_submission(&tx, &nowhere, 0), Err(FpSubmitError::Io { .. })));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The must-not-spend list is a safety list: an entry this cannot read is an ERROR. A selector
    /// that skipped it would spend the bond backing the claim it is submitting (audit3 H3/H12).
    #[test]
    fn an_unreadable_locked_outpoint_is_never_silently_dropped() {
        let good = format!("{}:7", "aa".repeat(64));
        assert_eq!(parse_locked_outpoints(std::slice::from_ref(&good)).unwrap().len(), 1);
        for bad in ["aa".repeat(64), format!("{}:x", "aa".repeat(64)), "nonsense:0".to_string()] {
            assert!(parse_locked_outpoints(&[good.clone(), bad.clone()]).is_err(), "{bad:?} must not shorten the set");
        }
    }
}
