//! **Verified remote solo, honestly labelled** (RFC-0009 §6 and the user's ruling of 2026-10-08: "not running a full node" is not
//! "trusting what a node says"; only the second is dangerous).
//!
//! Three verification layers, kept apart and labelled apart:
//!
//! * **L1 — header/DAG** ([`verify_header_chain_v1`]): from a checkpoint the client trusts BEFORE it talks to any node, a chain of
//!   headers whose hashes recompute, each listing the previous as a parent, blue work and blue score strictly rising, DAA never falling,
//!   no timestamp in the future, every PALW carriage the header carries well-formed and signed by the key it names (an attempt's
//!   challenge recomputed from the header's own position), a fresh tip. Independent views are compared by **containment only**
//!   ([`merge_views_v1`]): two views that conflict STOP the client — blue work is a download-ordering hint on MISAKA, never chain authority.
//! * **L2 — PALW fork choice**: the canonical chain is `palw_fork_choice::compare_palw_candidates_v1` over (safe frontier, safe weight,
//!   live total, hash), computed from ACCEPTED TRANSACTIONS and PALW state. A header cannot carry it, a state proof shows a row under a
//!   root but not that the chain carrying the root wins, and the transition leading to the root is not re-executed here. So L2 is
//!   established in exactly one way in this module: the facts are proven at a checkpoint the client already trusts (its own node, or a
//!   signed checkpoint it verified), which IS the decision point. Anything else is [`L2StatusV1::Unverified`] — a DESIGN_GAP, not a
//!   rounding error (record: `docs/design/palw/rfc-0009-remote-record.md`, round 2).
//! * **L3 — claim state** ([`proven_on_chain_v1`]): bond, class and claim rows proven (op 202) against a header that is ON the L1-verified
//!   chain — never against a header the client merely received.
//!
//! The four labels every client output carries ([`ModeLabelV1`]), and the one signing gate ([`signing_gate_v1`]): anything below
//! `VERIFIED_REMOTE` signs only on an explicit opt-in that names the class, and agreement among RPCs never raises a label.

use kaspa_consensus_core::header::Header;
use kaspa_consensus_core::palw_state_proof_v1::PalwFactProofV1;
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwBondStateV2, PalwBondStatusV2, PalwClassStateV2};
use kaspa_hashes::Hash64;

use crate::trust::Labelled;

// ---------------------------------------------------------------------------------------------------------------------------------
// The labels
// ---------------------------------------------------------------------------------------------------------------------------------

/// **The security class a client runs in** — printed on every output, in this spelling.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ModeLabelV1 {
    /// Facts read from nodes, nothing proven (however many agree).
    UnverifiedRemote,
    /// L1 (and, where facts were needed, L3) verified from a trusted checkpoint; the PALW fork choice (L2) is NOT — a valid-header
    /// branch that loses the economic order, or a correctly proven non-canonical state, cannot be told apart here.
    HeaderVerifiedForkChoiceUnverified,
    /// L1 + L2 + L3: the facts are proven at a trusted, fresh checkpoint that is the decision point. Communication, censorship and
    /// availability risks remain; it is not "a full node".
    VerifiedRemote,
    /// The client's own full node.
    FullNode,
}

impl ModeLabelV1 {
    pub const ALL: [ModeLabelV1; 4] = [
        ModeLabelV1::FullNode,
        ModeLabelV1::VerifiedRemote,
        ModeLabelV1::HeaderVerifiedForkChoiceUnverified,
        ModeLabelV1::UnverifiedRemote,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            ModeLabelV1::FullNode => "FULL_NODE",
            ModeLabelV1::VerifiedRemote => "VERIFIED_REMOTE",
            ModeLabelV1::HeaderVerifiedForkChoiceUnverified => "HEADER_VERIFIED_FORK_CHOICE_UNVERIFIED",
            ModeLabelV1::UnverifiedRemote => "UNVERIFIED_REMOTE",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|l| l.as_str().eq_ignore_ascii_case(text.trim()))
    }

    /// What may be claimed in this class — and, as importantly, what may not. Never says "full-node equivalent" below `FULL_NODE`.
    pub fn claim(&self) -> &'static str {
        match self {
            ModeLabelV1::FullNode => "your own full node verified this state",
            ModeLabelV1::VerifiedRemote => {
                "proven at your trusted checkpoint: safe under the same consensus rules; communication, censorship and availability risks remain"
            }
            ModeLabelV1::HeaderVerifiedForkChoiceUnverified => {
                "headers verified from your checkpoint, PALW fork choice NOT verified: this may be a valid-looking branch that is not the \
                 canonical chain — not full-node equivalent"
            }
            ModeLabelV1::UnverifiedRemote => {
                "reported by node(s), nothing proven — NOT full-node equivalent, however many nodes agree"
            }
        }
    }

    /// One line for a screen: `mode: <LABEL> — <claim>`.
    pub fn line(&self) -> String {
        format!("mode: {} — {}", self.as_str(), self.claim())
    }
}

impl std::fmt::Display for ModeLabelV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// **The one signing gate.** `FULL_NODE` and `VERIFIED_REMOTE` sign; a weaker class signs only when the user named it (or a weaker one)
/// with `--accept-unverified-state <LABEL>`. Asked before an expensive inference starts AND again right before the signature.
pub fn signing_gate_v1(label: ModeLabelV1, accepted: Option<ModeLabelV1>) -> Result<(), GateRefusalV1> {
    if label >= ModeLabelV1::VerifiedRemote {
        return Ok(());
    }
    match accepted {
        Some(floor) if floor <= label => Ok(()),
        _ => Err(GateRefusalV1 { label }),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error(
    "STOP: this client is in {label} ({}). Nothing was signed or started. To proceed anyway, accept the class explicitly: \
     --accept-unverified-state {label}",
    label.claim()
)]
pub struct GateRefusalV1 {
    pub label: ModeLabelV1,
}

// ---------------------------------------------------------------------------------------------------------------------------------
// The ruleset the client ships
// ---------------------------------------------------------------------------------------------------------------------------------

/// **The ruleset this client was built for** — the network, its genesis, `Params::consensus_params_id` and `consensus_schedule_id`.
/// The client never adopts a node's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClientRulesetV1 {
    pub network_id: String,
    pub genesis: String,
    pub consensus_params_id: String,
    pub consensus_schedule_id: String,
}

impl ClientRulesetV1 {
    pub fn of(params: &kaspa_consensus_core::config::params::Params) -> Self {
        Self {
            network_id: params.net.to_string(),
            genesis: params.genesis.hash.to_string(),
            consensus_params_id: params.consensus_params_id().to_string(),
            consensus_schedule_id: params.consensus_schedule_id().to_string(),
        }
    }
}

/// What a node says it runs (`getInfo`'s network, `getPalwNodeStatus`'s two ids).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeRulesetV1 {
    pub network_id: String,
    pub genesis: Option<String>,
    pub consensus_params_id: String,
    pub consensus_schedule_id: String,
}

/// A node on another ruleset is refused: its headers, templates and proofs are judged under rules this client does not run.
pub fn check_ruleset_v1(ours: &ClientRulesetV1, node: &NodeRulesetV1) -> Result<(), VerifyErrorV1> {
    if node.network_id != ours.network_id {
        return Err(VerifyErrorV1::WrongRuleset { what: "network", ours: ours.network_id.clone(), theirs: node.network_id.clone() });
    }
    if let Some(g) = &node.genesis
        && *g != ours.genesis
    {
        return Err(VerifyErrorV1::WrongRuleset { what: "genesis", ours: ours.genesis.clone(), theirs: g.clone() });
    }
    if node.consensus_params_id != ours.consensus_params_id {
        return Err(VerifyErrorV1::WrongRuleset {
            what: "consensus_params_id",
            ours: ours.consensus_params_id.clone(),
            theirs: node.consensus_params_id.clone(),
        });
    }
    if node.consensus_schedule_id != ours.consensus_schedule_id {
        return Err(VerifyErrorV1::WrongRuleset {
            what: "consensus_schedule_id (fence schedule)",
            ours: ours.consensus_schedule_id.clone(),
            theirs: node.consensus_schedule_id.clone(),
        });
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------------------------------------------
// L1
// ---------------------------------------------------------------------------------------------------------------------------------

/// Where the checkpoint's trust comes from — the client's configuration, never a node's word.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CheckpointTrustV1 {
    /// The user's own full node gave it.
    OwnNode,
    /// A signed checkpoint, verified against keys the client holds (`checkpoint::verify_signed_checkpoint`), issued at this DAA.
    Signed { issued_at_daa: u64 },
    /// The user typed it in (from somewhere they trust). Trusted as an anchor for L1; it establishes L2 only when fresh.
    UserPinned,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrustedCheckpointV1 {
    pub block: Hash64,
    pub daa_score: u64,
    pub trust: CheckpointTrustV1,
}

/// The bounds a verification runs under (the client's own, never a node's).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VerifyLimitsV1 {
    /// A header timestamp may be at most this far ahead of the client's clock.
    pub max_future_ms: u64,
    /// The tip must be at most this old by the client's clock — a node that stopped serving new blocks is a stale view.
    pub max_tip_age_ms: u64,
    /// The longest header chain the client accepts from one checkpoint (a fresh checkpoint keeps this short; a node cannot make the client
    /// walk forever).
    pub max_headers: usize,
    /// A checkpoint establishes L2 only while the tip is at most this many DAA past it.
    pub max_checkpoint_lag_daa: u64,
}

impl Default for VerifyLimitsV1 {
    fn default() -> Self {
        // testnet-12 runs ~120 s per DAA: 10 minutes ahead, 30 minutes of silence, ~a day of headers, one block of checkpoint lag.
        Self { max_future_ms: 600_000, max_tip_age_ms: 1_800_000, max_headers: 1_000, max_checkpoint_lag_daa: 1 }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum VerifyErrorV1 {
    #[error("the node runs another {what}: ours {ours}, theirs {theirs}")]
    WrongRuleset { what: &'static str, ours: String, theirs: String },
    #[error("no headers were served")]
    Empty,
    #[error("the chain does not start at the trusted checkpoint {expected} (it starts at {got})")]
    NotFromCheckpoint { expected: Hash64, got: Hash64 },
    #[error("the checkpoint header's DAA {got} is not the checkpoint's {expected}")]
    CheckpointDaaMismatch { expected: u64, got: u64 },
    #[error("header #{at} does not hash to the hash it carries")]
    HeaderHashMismatch { at: usize },
    #[error("header #{at} does not list the previous header as a parent: the chain is not linked")]
    Unlinked { at: usize },
    #[error("header #{at}: {field} does not rise along the chain")]
    NotMonotone { at: usize, field: &'static str },
    #[error("header #{at} carries an unknown algorithm id {algo}")]
    UnknownAlgo { at: usize, algo: u8 },
    #[error("header #{at}'s PALW carriage is not well-formed and signed by the key it names: {why}")]
    BadCarriage { at: usize, why: String },
    #[error("header #{at} is stamped {ahead_ms} ms in the future")]
    FromTheFuture { at: usize, ahead_ms: u64 },
    #[error("the served tip is {age_ms} ms old (the bound is {max_ms}): a stale view")]
    StaleTip { age_ms: u64, max_ms: u64 },
    #[error("{0} headers is more than this client walks from one checkpoint ({1}): get a fresher checkpoint")]
    TooLong(usize, usize),
    #[error(
        "two independently verified views conflict (they part after {fork}): the PALW fork choice that would decide is not verified here — STOP"
    )]
    ConflictingForks { fork: Hash64 },
    #[error("the views are not rooted at the same checkpoint")]
    DifferentCheckpoints,
    #[error("the template's parent {parent} is not the verified tip {tip}: a stale or foreign parent")]
    StaleParent { parent: Hash64, tip: Hash64 },
    #[error("the template's target (bits {template:#x}) is not within a factor of 2 of the verified tip's (bits {tip:#x})")]
    WrongTarget { template: u32, tip: u32 },
    #[error("the proof's header {0} is not on the verified chain")]
    ProofHeaderOffChain(Hash64),
    #[error("the state proof does not hold: {0}")]
    Proof(String),
    #[error("a node said {what}, the proof at the verified header says otherwise — the node is lying or stale")]
    ContradictsProof { what: String },
}

/// What L1 established: the chain from the checkpoint to the tip, by hash, and what was NOT checked (stated, never implied).
#[derive(Clone, Debug)]
pub struct VerifiedChainV1 {
    pub checkpoint: Hash64,
    pub hashes: Vec<Hash64>,
    pub tip: Header,
    pub not_checked: Vec<&'static str>,
    /// Each block's point and the root its header commits (its selected parent's post-state) — what L2 checks an opening and an
    /// attestation against. Index-aligned with `hashes`; each block's predecessor here is its selected parent.
    pub points: Vec<ChainPointV1>,
}

/// One verified block, as L2 reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChainPointV1 {
    pub hash: Hash64,
    pub daa_score: u64,
    pub blue_score: u64,
    /// What this header commits: the root of its PREDECESSOR's post-state.
    pub palw_state_root: Hash64,
}

/// Two verified chains are the same chain when they name the same blocks from the same checkpoint (the header bytes are fixed by the hashes).
impl PartialEq for VerifiedChainV1 {
    fn eq(&self, other: &Self) -> bool {
        self.checkpoint == other.checkpoint && self.hashes == other.hashes && self.tip.hash == other.tip.hash
    }
}
impl Eq for VerifiedChainV1 {}

impl VerifiedChainV1 {
    pub fn tip_hash(&self) -> Hash64 {
        self.tip.hash
    }
    pub fn contains(&self, block: &Hash64) -> bool {
        self.hashes.contains(block)
    }
    pub fn index_of(&self, block: &Hash64) -> Option<usize> {
        self.hashes.iter().position(|h| h == block)
    }
}

/// The checks L1 does NOT make, named on every verified chain.
pub const L1_NOT_CHECKED_V1: [&str; 3] = [
    "heartbeat PoW and bits against the DAA window (the window's mergesets are not served)",
    "GHOSTDAG selected-parent choice (the other parents' headers are not served)",
    "the PALW fork choice and the state transition (L2)",
];

fn verify_carriage_v1(header: &Header, network_domain: Hash64) -> Result<(), String> {
    use kaspa_consensus_core::pow_layer0::{POW_ALGO_ID_PALW_RECEIPT_V3, is_palw_attempt_algo_id};
    let verify = |key: &[u8], message: &[u8], sig: &[u8], context: &[u8]| {
        kaspa_txscript::verify_mldsa87_with_context(key, message, sig, context).unwrap_or(false)
    };
    if is_palw_attempt_algo_id(header.pow_algo_id) {
        use kaspa_consensus_core::palw_attempt_v2::{PalwAttemptEnvelopeV2, challenge_v2};
        let envelope = PalwAttemptEnvelopeV2::decode_wire(&header.palw_commitment).map_err(|e| e.to_string())?;
        let a = &envelope.attempt;
        let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(header);
        if a.network_domain != network_domain {
            return Err("the attempt names another network".into());
        }
        if a.challenge != challenge_v2(network_domain, pre_pow, header.timestamp, header.nonce, a.class_id, &a.executor_bond) {
            return Err("the attempt's challenge does not bind this header's position".into());
        }
        envelope.validate_signature_v2(verify).map_err(|e| e.to_string())?;
    } else if header.pow_algo_id == POW_ALGO_ID_PALW_RECEIPT_V3 {
        let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(header);
        if kaspa_consensus_core::palw_receipt_v4::palw_receipt_v4_carriage_is_v4(&header.palw_commitment) {
            let e = kaspa_consensus_core::palw_receipt_v4::PalwReceiptSpendEnvelopeV4::decode(&header.palw_commitment)
                .map_err(|e| e.to_string())?;
            e.validate_stateless_v4(network_domain, pre_pow, header.timestamp, header.nonce).map_err(|e| e.to_string())?;
            e.validate_signatures_v4(verify).map_err(|e| e.to_string())?;
        } else {
            let e = kaspa_consensus_core::palw_freeprompt_v3::PalwReceiptSpendEnvelopeV3::decode(&header.palw_commitment)
                .map_err(|e| e.to_string())?;
            e.validate_stateless_v3(network_domain, pre_pow, header.timestamp, header.nonce).map_err(|e| e.to_string())?;
            e.validate_signature_v3(verify).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// **L1: a header chain from a trusted checkpoint to a tip.** `headers[0]` is the checkpoint's header; each next one names the previous as
/// a parent. Pure: a restarted client asking another peer recomputes everything and carries no trust from the first.
pub fn verify_header_chain_v1(
    checkpoint: &TrustedCheckpointV1,
    headers: &[Header],
    network_domain: Hash64,
    now_ms: u64,
    limits: &VerifyLimitsV1,
) -> Result<VerifiedChainV1, VerifyErrorV1> {
    let first = headers.first().ok_or(VerifyErrorV1::Empty)?;
    if headers.len() > limits.max_headers {
        return Err(VerifyErrorV1::TooLong(headers.len(), limits.max_headers));
    }
    let mut hashes = Vec::with_capacity(headers.len());
    let mut points = Vec::with_capacity(headers.len());
    for (at, h) in headers.iter().enumerate() {
        if kaspa_consensus_core::hashing::header::hash(h) != h.hash {
            return Err(VerifyErrorV1::HeaderHashMismatch { at });
        }
        if at == 0 {
            if h.hash != checkpoint.block {
                return Err(VerifyErrorV1::NotFromCheckpoint { expected: checkpoint.block, got: h.hash });
            }
            if h.daa_score != checkpoint.daa_score {
                return Err(VerifyErrorV1::CheckpointDaaMismatch { expected: checkpoint.daa_score, got: h.daa_score });
            }
        } else {
            let prev = &headers[at - 1];
            if !h.direct_parents().contains(&prev.hash) {
                return Err(VerifyErrorV1::Unlinked { at });
            }
            if h.blue_work <= prev.blue_work {
                return Err(VerifyErrorV1::NotMonotone { at, field: "blue work" });
            }
            if h.blue_score <= prev.blue_score {
                return Err(VerifyErrorV1::NotMonotone { at, field: "blue score" });
            }
            if h.daa_score < prev.daa_score {
                return Err(VerifyErrorV1::NotMonotone { at, field: "DAA score" });
            }
        }
        kaspa_consensus_core::pow_layer0::check_algo_id_known(h.pow_algo_id)
            .map_err(|_| VerifyErrorV1::UnknownAlgo { at, algo: h.pow_algo_id })?;
        // The checkpoint's own carriage is the checkpoint's; every header after it must stand on its own.
        if at > 0 {
            verify_carriage_v1(h, network_domain).map_err(|why| VerifyErrorV1::BadCarriage { at, why })?;
        }
        if h.timestamp > now_ms.saturating_add(limits.max_future_ms) {
            return Err(VerifyErrorV1::FromTheFuture { at, ahead_ms: h.timestamp - now_ms });
        }
        hashes.push(h.hash);
        points.push(ChainPointV1 {
            hash: h.hash,
            daa_score: h.daa_score,
            blue_score: h.blue_score,
            palw_state_root: h.palw_state_root,
        });
    }
    let tip = headers.last().expect("non-empty").clone();
    let age = now_ms.saturating_sub(tip.timestamp);
    if age > limits.max_tip_age_ms {
        return Err(VerifyErrorV1::StaleTip { age_ms: age, max_ms: limits.max_tip_age_ms });
    }
    let _ = first;
    Ok(VerifiedChainV1 { checkpoint: checkpoint.block, hashes, tip, not_checked: L1_NOT_CHECKED_V1.to_vec(), points })
}

/// **Independent views, by containment only.** One extends the other → the longer is kept; they part → [`VerifyErrorV1::ConflictingForks`],
/// whatever either view's blue work: on MISAKA the header blue work is a download-ordering hint, and the order that decides (L2) is not
/// verified here. A peer that hides a competing tip is caught exactly when another peer shows it.
pub fn merge_views_v1(views: &[VerifiedChainV1]) -> Result<VerifiedChainV1, VerifyErrorV1> {
    let mut best = views.first().ok_or(VerifyErrorV1::Empty)?.clone();
    for v in &views[1..] {
        if v.checkpoint != best.checkpoint {
            return Err(VerifyErrorV1::DifferentCheckpoints);
        }
        if best.contains(&v.tip_hash()) {
            continue;
        }
        if v.contains(&best.tip_hash()) {
            best = v.clone();
            continue;
        }
        let fork =
            best.hashes.iter().zip(v.hashes.iter()).take_while(|(a, b)| a == b).last().map(|(a, _)| *a).unwrap_or(best.checkpoint);
        return Err(VerifyErrorV1::ConflictingForks { fork });
    }
    Ok(best)
}

/// A compact target's magnitude (mantissa × 256^(exponent−3)), as a float — for a factor-of-two bound, not for consensus.
fn compact_target_f64(bits: u32) -> f64 {
    let exponent = (bits >> 24) as i32;
    let mantissa = (bits & 0x007f_ffff) as f64;
    mantissa * 256f64.powi(exponent - 3)
}

/// **A template, checked against the verified chain** before any inference: its parents must include the verified tip (a template
/// built on another, stale or hidden branch is refused), and its target must be within a factor of two of the tip's (the exact value needs
/// the DAA window, which L1 does not have — a node that serves a far easier or harder target is refused, a subtle one is not caught).
pub fn check_template_v1(template: &Header, chain: &VerifiedChainV1) -> Result<(), VerifyErrorV1> {
    let tip = chain.tip_hash();
    if !template.direct_parents().contains(&tip) {
        let parent = template.direct_parents().first().copied().unwrap_or_default();
        return Err(VerifyErrorV1::StaleParent { parent, tip });
    }
    let (t, c) = (compact_target_f64(template.bits), compact_target_f64(chain.tip.bits));
    if !(c > 0.0 && t >= c / 2.0 && t <= c * 2.0) {
        return Err(VerifyErrorV1::WrongTarget { template: template.bits, tip: chain.tip.bits });
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------------------------------------------
// L3 and L2
// ---------------------------------------------------------------------------------------------------------------------------------

/// The header a proof is checked against must be on the verified chain (never a header the client merely received).
fn on_chain(chain: &VerifiedChainV1, proof_header: &Header) -> Result<(), VerifyErrorV1> {
    if kaspa_consensus_core::hashing::header::hash(proof_header) != proof_header.hash || !chain.contains(&proof_header.hash) {
        return Err(VerifyErrorV1::ProofHeaderOffChain(proof_header.hash));
    }
    Ok(())
}

/// **L3: a bond, proven at a header on the verified chain.** `None` = proven absent.
pub fn bond_on_chain_v1(
    chain: &VerifiedChainV1,
    proof_header: &Header,
    proof: &PalwFactProofV1,
    bond: &PalwBondKeyV2,
) -> Result<Labelled<Option<PalwBondStateV2>>, VerifyErrorV1> {
    on_chain(chain, proof_header)?;
    crate::proof::bond_at_pin_v1(proof_header, proof_header.hash, proof, bond).map_err(|e| VerifyErrorV1::Proof(e.to_string()))
}

/// **L3: a class, proven at a header on the verified chain.** `None` = proven absent.
pub fn class_on_chain_v1(
    chain: &VerifiedChainV1,
    proof_header: &Header,
    proof: &PalwFactProofV1,
    class_id: &Hash64,
) -> Result<Labelled<Option<PalwClassStateV2>>, VerifyErrorV1> {
    on_chain(chain, proof_header)?;
    crate::proof::class_at_pin_v1(proof_header, proof_header.hash, proof, class_id).map_err(|e| VerifyErrorV1::Proof(e.to_string()))
}

/// **The ADR-0043 root a header on the verified chain commits, for L3** — the header's own root below
/// `palw_fork_choice_commitment_v1`; past it the header commits `H(leaf ‖ root)` of its predecessor's post-state, and the predecessor's
/// opening (op 203, untrusted until it hashes to the header's root and names the predecessor) unwraps it. Header-bound, exactly as L3
/// always was: whether the header's root is the fold of its chain is L2's question (`crate::l2::l3_root_under_l2_v1` answers both).
pub fn l3_root_at_header_v1(
    chain: &VerifiedChainV1,
    proof_header: &Header,
    openings: &[kaspa_consensus_core::palw_fork_choice_commitment_v1::PalwForkChoiceOpeningV1],
    fence: Option<kaspa_consensus_core::config::params::ForkActivation>,
) -> Result<Hash64, VerifyErrorV1> {
    use kaspa_consensus_core::palw_fork_choice_commitment_v1::{PalwForkChoicePointV1, palw_fork_choice_committed_at_v1};
    on_chain(chain, proof_header)?;
    let at = chain.index_of(&proof_header.hash).ok_or(VerifyErrorV1::ProofHeaderOffChain(proof_header.hash))?;
    let Some(parent) = at.checked_sub(1).map(|i| chain.points[i]) else {
        return Err(VerifyErrorV1::Proof(
            "the checkpoint's own header commits a state before the view: prove at a later header".into(),
        ));
    };
    if !palw_fork_choice_committed_at_v1(fence, Some(parent.daa_score)) {
        return Ok(proof_header.palw_state_root);
    }
    let point = PalwForkChoicePointV1 { block: parent.hash, daa_score: parent.daa_score, blue_score: parent.blue_score };
    let mut why = format!("no opening of {} was served", parent.hash);
    for o in openings.iter().filter(|o| o.leaf.block == parent.hash) {
        match o.verify(&proof_header.palw_state_root, &point, fence) {
            Ok(_) => return Ok(o.inner_root),
            Err(e) => why = e.to_string(),
        }
    }
    Err(VerifyErrorV1::Proof(format!(
        "past the fork-choice commitment the header {} commits an envelope, and it was not opened: {why}",
        proof_header.hash
    )))
}

/// **L3 under an established root**: a bond, proven at a header on the verified chain against `root` (from [`l3_root_at_header_v1`] or
/// `crate::l2::l3_root_under_l2_v1`). `None` = proven absent.
pub fn bond_on_chain_under_v1(
    chain: &VerifiedChainV1,
    proof_header: &Header,
    root: Hash64,
    proof: &PalwFactProofV1,
    bond: &PalwBondKeyV2,
) -> Result<Labelled<Option<PalwBondStateV2>>, VerifyErrorV1> {
    on_chain(chain, proof_header)?;
    crate::proof::bond_under_root_v1(root, proof_header.hash, proof_header.daa_score, proof, bond)
        .map_err(|e| VerifyErrorV1::Proof(e.to_string()))
}

/// **L3 under an established root**: a class (see [`bond_on_chain_under_v1`]). `None` = proven absent.
pub fn class_on_chain_under_v1(
    chain: &VerifiedChainV1,
    proof_header: &Header,
    root: Hash64,
    proof: &PalwFactProofV1,
    class_id: &Hash64,
) -> Result<Labelled<Option<PalwClassStateV2>>, VerifyErrorV1> {
    on_chain(chain, proof_header)?;
    crate::proof::class_under_root_v1(root, proof_header.hash, proof_header.daa_score, proof, class_id)
        .map_err(|e| VerifyErrorV1::Proof(e.to_string()))
}

/// What a node SAID about the job (its template facts, its class row), compared with what is proven. A mismatch stops everything.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeJobFactsV1 {
    pub class_id: Hash64,
    pub artifact_root: Hash64,
    pub bond: PalwBondKeyV2,
    pub bond_pubkey: Vec<u8>,
    pub bond_may_produce: bool,
}

/// **The job a node offered, against the proven rows.** The class must be present with the node's artifact root; the bond must be present,
/// hold the key, and not be retiring. Refused before execution and before signing.
pub fn check_job_facts_v1(
    said: &NodeJobFactsV1,
    class: &Labelled<Option<PalwClassStateV2>>,
    bond: &Labelled<Option<PalwBondStateV2>>,
) -> Result<(), VerifyErrorV1> {
    let Some(c) = &class.value else {
        return Err(VerifyErrorV1::ContradictsProof { what: format!("class {} exists", said.class_id) });
    };
    if c.artifact_root != said.artifact_root {
        return Err(VerifyErrorV1::ContradictsProof {
            what: format!("class {}'s artifact root is {} (proven: {})", said.class_id, said.artifact_root, c.artifact_root),
        });
    }
    let Some(b) = &bond.value else {
        return Err(VerifyErrorV1::ContradictsProof { what: format!("bond {:?} exists", said.bond) });
    };
    if b.pubkey != said.bond_pubkey {
        return Err(VerifyErrorV1::ContradictsProof { what: "the bond holds this key".into() });
    }
    let retiring = matches!(b.status, PalwBondStatusV2::Retiring { .. });
    if said.bond_may_produce && retiring {
        return Err(VerifyErrorV1::ContradictsProof { what: "the bond may produce (proven: retiring)".into() });
    }
    if retiring {
        return Err(VerifyErrorV1::ContradictsProof { what: "the bond is retiring and may take no new claims".into() });
    }
    Ok(())
}

/// **L2's status.** Established only when the facts were proven at the trusted checkpoint itself, the checkpoint is the client's own node
/// or a verified signed checkpoint (a typed-in pin is an L1 anchor, not an L2 authority), it is the decision point (the verified tip is at
/// most `max_checkpoint_lag_daa` past it) — and no conflicting view was seen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum L2StatusV1 {
    EstablishedAtTrustedCheckpoint,
    /// RFC-0009 L2 (`crate::l2`): the comparator's inputs opened by this client from a root an issuer it trusts attested, and the chosen
    /// tip robustly dominating every candidate the configured peers show. `trust` is the line every output prints.
    EstablishedByAttestation {
        trust: String,
    },
    Unverified(&'static str),
}

pub fn l2_status_v1(
    checkpoint: &TrustedCheckpointV1,
    chain: &VerifiedChainV1,
    facts_proven_at: Option<Hash64>,
    limits: &VerifyLimitsV1,
) -> L2StatusV1 {
    if matches!(checkpoint.trust, CheckpointTrustV1::UserPinned) {
        return L2StatusV1::Unverified("a typed-in pin anchors the headers; it is not a fork-choice authority");
    }
    if facts_proven_at != Some(checkpoint.block) {
        return L2StatusV1::Unverified(
            "the facts are proven at a header past the trusted checkpoint: the transition and fork choice that lead there are not verified",
        );
    }
    if chain.tip.daa_score > checkpoint.daa_score.saturating_add(limits.max_checkpoint_lag_daa) {
        return L2StatusV1::Unverified("the trusted checkpoint is no longer the decision point (the chain moved past it)");
    }
    L2StatusV1::EstablishedAtTrustedCheckpoint
}

/// **The label, from what was verified** — never from how many nodes agreed.
pub fn mode_label_v1(own_full_node: bool, l1: bool, l3: bool, l2: &L2StatusV1) -> ModeLabelV1 {
    if own_full_node {
        return ModeLabelV1::FullNode;
    }
    match (l1, l3, l2) {
        (true, true, L2StatusV1::EstablishedAtTrustedCheckpoint | L2StatusV1::EstablishedByAttestation { .. }) => {
            ModeLabelV1::VerifiedRemote
        }
        (true, true, _) => ModeLabelV1::HeaderVerifiedForkChoiceUnverified,
        _ => ModeLabelV1::UnverifiedRemote,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_state_proof_v1::{prove_bonds_v1, prove_classes_v1};
    use kaspa_consensus_core::palw_state_v2::{
        PalwBlockContextV2, PalwChainStateV2, PalwConsensusObjectV2 as Obj, PalwPwuRuleV2, PalwStateParamsV2, apply_palw_transition_v2,
    };
    use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

    const NOW: u64 = 1_800_000_000_000;
    const STEP: u64 = 120_000;

    fn h(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }
    fn domain() -> Hash64 {
        h(0xD0)
    }
    fn header(parents: Vec<Hash64>, daa: u64, blue: u64, ts: u64, state_root: Hash64, salt: u64) -> Header {
        let mut x = Header::new_finalized(
            1,
            vec![parents].try_into().unwrap(),
            h(2),
            h(3),
            h(4),
            ts,
            0x1d00_ffff,
            salt,
            kaspa_consensus_core::pow_layer0::POW_ALGO_ID_HEARTBEAT_V1,
            daa,
            (blue as u64).into(),
            blue,
            h(5),
        )
        .with_palw_state_root(state_root);
        x.finalize();
        x
    }
    /// A chain of `n` heartbeats after the checkpoint `c`, every one committing `root`; `salt` makes a sibling branch.
    fn extend(from: &Header, n: usize, root: Hash64, salt: u64) -> Vec<Header> {
        let mut out = vec![from.clone()];
        for i in 0..n {
            let p = out.last().unwrap().clone();
            out.push(header(vec![p.hash], p.daa_score + 1, p.blue_score + 1, p.timestamp + STEP, root, salt * 1000 + i as u64));
        }
        out
    }
    fn checkpoint_header(root: Hash64) -> Header {
        header(vec![h(1)], 100, 100, NOW - 10 * STEP, root, 0)
    }
    fn trusted(c: &Header, trust: CheckpointTrustV1) -> TrustedCheckpointV1 {
        TrustedCheckpointV1 { block: c.hash, daa_score: c.daa_score, trust }
    }
    fn lim() -> VerifyLimitsV1 {
        VerifyLimitsV1::default()
    }

    /// A state with one class and one Active bond (key `[7; 2592]`), and its proofs.
    fn fixture_state(retiring: bool) -> (PalwChainStateV2, PalwBondKeyV2, Hash64) {
        let params = PalwStateParamsV2::new(100, 1, 1, 1, 500, 1_000, h(1), 4, 1_000, 100, 1_000, 0).unwrap();
        let bond = PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_u64_word(0xB0), 0));
        let class = h(1); // the base class (the first class a state registers)
        let objects = vec![
            Obj::ClassRegistered {
                class_id: class,
                artifact_root: h(0xA1),
                slash_value_per_pwu: 5,
                pwu_rule: PalwPwuRuleV2::MaxPerAttempt(160),
                initial_target: u128::MAX / 2,
                share_permille: 1000,
                activation_daa: 0,
                admission: None,
            },
            Obj::BondRegistered {
                bond,
                pubkey: vec![7; 2592],
                operator_pubkey: vec![8; 8],
                collateral: 1 << 40,
                payout_payload: h(0x9A),
                capable_classes: Default::default(),
                signature: Vec::new(),
            },
        ];
        let cx = PalwBlockContextV2 { block: h(10), daa_score: 1, blue_score: 1, subsidy: 0 };
        let (mut state, _) = apply_palw_transition_v2(&PalwChainStateV2::genesis(), &params, &cx, &objects, None).unwrap();
        if retiring {
            let cx2 = PalwBlockContextV2 { block: h(11), daa_score: 2, blue_score: 2, subsidy: 0 };
            let retire = Obj::BondRetireRequested { bond, signature: Vec::new() };
            if let Ok((s, _)) = apply_palw_transition_v2(&state, &params, &cx2, &[retire], None) {
                state = s;
            }
        }
        (state, bond, class)
    }

    fn job(bond: PalwBondKeyV2, class: Hash64) -> NodeJobFactsV1 {
        NodeJobFactsV1 { class_id: class, artifact_root: h(0xA1), bond, bond_pubkey: vec![7; 2592], bond_may_produce: true }
    }

    #[test]
    fn the_labels_are_spelled_once_and_never_claim_full_node_equivalence_below_full_node() {
        for l in ModeLabelV1::ALL {
            assert_eq!(ModeLabelV1::parse(l.as_str()), Some(l));
            if l != ModeLabelV1::FullNode {
                assert!(
                    !l.claim().contains("equivalent")
                        || l.claim().contains("not full-node equivalent")
                        || l.claim().contains("NOT full-node")
                );
            }
        }
        assert!(ModeLabelV1::UnverifiedRemote.line().starts_with("mode: UNVERIFIED_REMOTE"));
        // The gate: below VERIFIED_REMOTE, only the named class (or a weaker one) opens it.
        assert!(signing_gate_v1(ModeLabelV1::VerifiedRemote, None).is_ok());
        assert!(signing_gate_v1(ModeLabelV1::FullNode, None).is_ok());
        let refused = signing_gate_v1(ModeLabelV1::HeaderVerifiedForkChoiceUnverified, None).unwrap_err();
        assert!(refused.to_string().contains("--accept-unverified-state HEADER_VERIFIED_FORK_CHOICE_UNVERIFIED"));
        assert!(
            signing_gate_v1(ModeLabelV1::HeaderVerifiedForkChoiceUnverified, Some(ModeLabelV1::HeaderVerifiedForkChoiceUnverified))
                .is_ok()
        );
        assert!(signing_gate_v1(ModeLabelV1::HeaderVerifiedForkChoiceUnverified, Some(ModeLabelV1::UnverifiedRemote)).is_ok());
        assert!(
            signing_gate_v1(ModeLabelV1::UnverifiedRemote, Some(ModeLabelV1::HeaderVerifiedForkChoiceUnverified)).is_err(),
            "accepting the header-verified class does not accept the unverified one"
        );
        assert!(signing_gate_v1(ModeLabelV1::UnverifiedRemote, Some(ModeLabelV1::UnverifiedRemote)).is_ok());
        // Many agreeing RPCs never raise a label: the label is a function of what was verified.
        assert_eq!(mode_label_v1(false, false, false, &L2StatusV1::Unverified("x")), ModeLabelV1::UnverifiedRemote);
    }

    #[test]
    fn l1_accepts_a_linked_fresh_chain_and_names_what_it_did_not_check() {
        let c = checkpoint_header(h(0x51));
        let chain = extend(&c, 5, h(0x51), 1);
        let v = verify_header_chain_v1(&trusted(&c, CheckpointTrustV1::Signed { issued_at_daa: 100 }), &chain, domain(), NOW, &lim())
            .unwrap();
        assert_eq!(v.tip_hash(), chain.last().unwrap().hash);
        assert_eq!(v.hashes.len(), 6);
        assert!(v.not_checked.iter().any(|n| n.contains("fork choice")), "L1 says it did not check L2");
    }

    /// **Malicious-node fixtures, L1** — each refused before anything runs.
    #[test]
    fn l1_refuses_a_forged_unlinked_non_monotone_future_or_stale_chain() {
        let c = checkpoint_header(h(0x51));
        let t = trusted(&c, CheckpointTrustV1::OwnNode);
        let good = extend(&c, 4, h(0x51), 1);
        // not from the checkpoint
        let other = checkpoint_header(h(0x52));
        assert!(matches!(
            verify_header_chain_v1(&t, &extend(&other, 2, h(0x52), 1), domain(), NOW, &lim()),
            Err(VerifyErrorV1::NotFromCheckpoint { .. })
        ));
        // a header whose bytes were altered after hashing (a fake state root, say)
        let mut forged = good.clone();
        forged[2].palw_state_root = h(0xBAD);
        assert!(matches!(
            verify_header_chain_v1(&t, &forged, domain(), NOW, &lim()),
            Err(VerifyErrorV1::HeaderHashMismatch { at: 2 })
        ));
        // a proof against an unlinked header: a header that does not name the previous as a parent
        let mut unlinked = good.clone();
        unlinked[3] =
            header(vec![h(0x77)], unlinked[2].daa_score + 1, unlinked[2].blue_score + 1, unlinked[2].timestamp + STEP, h(0x51), 9);
        assert!(matches!(verify_header_chain_v1(&t, &unlinked, domain(), NOW, &lim()), Err(VerifyErrorV1::Unlinked { at: 3 })));
        // blue work not rising
        let mut flat = good.clone();
        let p = flat[1].clone();
        flat[2] = header(vec![p.hash], p.daa_score + 1, p.blue_score, p.timestamp + STEP, h(0x51), 8);
        assert!(matches!(verify_header_chain_v1(&t, &flat[..3], domain(), NOW, &lim()), Err(VerifyErrorV1::NotMonotone { .. })));
        // a header from the future
        let mut future = good.clone();
        let p = future[3].clone();
        future[4] = header(vec![p.hash], p.daa_score + 1, p.blue_score + 1, NOW + 3_600_000, h(0x51), 7);
        assert!(matches!(verify_header_chain_v1(&t, &future, domain(), NOW, &lim()), Err(VerifyErrorV1::FromTheFuture { at: 4, .. })));
        // a stale view: the served tip is hours old
        assert!(matches!(
            verify_header_chain_v1(&t, &good, domain(), NOW + 6 * 3_600_000, &lim()),
            Err(VerifyErrorV1::StaleTip { .. })
        ));
        // too long a walk from one checkpoint
        let long = extend(&c, 20, h(0x51), 1);
        let short = VerifyLimitsV1 { max_headers: 10, ..lim() };
        assert!(matches!(verify_header_chain_v1(&t, &long, domain(), NOW + 15 * STEP, &short), Err(VerifyErrorV1::TooLong(21, 10))));
        // an attempt header whose carriage is junk is refused (a carriage is inside the block identity)
        let mut junk = good.clone();
        let p = junk[3].clone();
        let mut a = header(vec![p.hash], p.daa_score + 1, p.blue_score + 1, p.timestamp + STEP, h(0x51), 6);
        a.pow_algo_id = kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_COMMITTED_V2;
        a.palw_commitment = vec![1, 2, 3];
        a.finalize();
        junk[4] = a;
        assert!(matches!(verify_header_chain_v1(&t, &junk, domain(), NOW, &lim()), Err(VerifyErrorV1::BadCarriage { at: 4, .. })));
    }

    #[test]
    fn a_node_on_another_ruleset_or_fence_schedule_is_refused() {
        let ours = ClientRulesetV1 {
            network_id: "testnet-12".into(),
            genesis: "g".into(),
            consensus_params_id: "p".into(),
            consensus_schedule_id: "s".into(),
        };
        let node = NodeRulesetV1 {
            network_id: "testnet-12".into(),
            genesis: Some("g".into()),
            consensus_params_id: "p".into(),
            consensus_schedule_id: "s".into(),
        };
        assert!(check_ruleset_v1(&ours, &node).is_ok());
        for bad in [
            NodeRulesetV1 { network_id: "testnet-11".into(), ..node.clone() },
            NodeRulesetV1 { genesis: Some("other".into()), ..node.clone() },
            NodeRulesetV1 { consensus_params_id: "p2".into(), ..node.clone() },
            NodeRulesetV1 { consensus_schedule_id: "s2 (a fence moved)".into(), ..node.clone() },
        ] {
            assert!(matches!(check_ruleset_v1(&ours, &bad), Err(VerifyErrorV1::WrongRuleset { .. })), "{bad:?}");
        }
    }

    /// **The fork that wins on header blue work and the peer that hides a tip** (the P2 correction's attack tests): the client never picks
    /// between conflicting views by blue work. Shown only the heavier branch it stays at HEADER_VERIFIED_FORK_CHOICE_UNVERIFIED and will not
    /// sign without the opt-in; shown both, it STOPS.
    #[test]
    fn conflicting_views_stop_the_client_whatever_their_blue_work() {
        let c = checkpoint_header(h(0x51));
        let t = trusted(&c, CheckpointTrustV1::Signed { issued_at_daa: 100 });
        let a = extend(&c, 3, h(0x51), 1); // the PALW-canonical branch (fewer headers)
        let b = extend(&c, 6, h(0x52), 2); // more blue work, but (say) less matured PALW weight
        let va = verify_header_chain_v1(&t, &a, domain(), NOW, &lim()).unwrap();
        let vb = verify_header_chain_v1(&t, &b, domain(), NOW + 3 * STEP, &lim()).unwrap();
        assert!(vb.tip.blue_work > va.tip.blue_work);
        // A peer that hides A: alone, B verifies at L1 — and the label does not go above header-verified.
        let alone = merge_views_v1(std::slice::from_ref(&vb)).unwrap();
        let l2 = l2_status_v1(&t, &alone, Some(alone.tip_hash()), &lim());
        assert!(matches!(l2, L2StatusV1::Unverified(_)));
        let label = mode_label_v1(false, true, true, &l2);
        assert_eq!(label, ModeLabelV1::HeaderVerifiedForkChoiceUnverified);
        assert!(signing_gate_v1(label, None).is_err(), "no signature on an unverified fork choice without the opt-in");
        // A second, independent peer reveals A: the views conflict and the client stops — it does not pick B for its blue work.
        assert!(matches!(merge_views_v1(&[vb.clone(), va.clone()]), Err(VerifyErrorV1::ConflictingForks { fork }) if fork == c.hash));
        assert!(matches!(merge_views_v1(&[va.clone(), vb]), Err(VerifyErrorV1::ConflictingForks { .. })));
        // A peer that is merely behind (A's prefix) is consistent: the longer view is kept.
        let behind = verify_header_chain_v1(&t, &a[..2], domain(), NOW, &lim()).unwrap();
        assert_eq!(merge_views_v1(&[behind, va.clone()]).unwrap().tip_hash(), va.tip_hash());
    }

    /// **Templates: a stale or foreign parent, and a wrong target**, refused before inference.
    #[test]
    fn a_template_off_the_verified_tip_or_with_a_wrong_target_is_refused() {
        let c = checkpoint_header(h(0x51));
        let t = trusted(&c, CheckpointTrustV1::OwnNode);
        let chain = extend(&c, 3, h(0x51), 1);
        let v = verify_header_chain_v1(&t, &chain, domain(), NOW, &lim()).unwrap();
        let tip = chain.last().unwrap();
        let ok = header(vec![tip.hash], tip.daa_score + 1, tip.blue_score + 1, tip.timestamp + STEP, h(0x51), 50);
        assert!(check_template_v1(&ok, &v).is_ok());
        // a stale parent (the tip's parent) — or a job taken right after a reorg, whose parent the re-checked view no longer holds
        let stale = header(vec![chain[2].hash], tip.daa_score, tip.blue_score, tip.timestamp, h(0x51), 51);
        assert!(matches!(check_template_v1(&stale, &v), Err(VerifyErrorV1::StaleParent { .. })));
        // a far easier target (bits exponent +1 = 256×)
        let mut easy = ok.clone();
        easy.bits = 0x1e00_ffff;
        easy.finalize();
        assert!(matches!(check_template_v1(&easy, &v), Err(VerifyErrorV1::WrongTarget { .. })));
        // after a reorg the re-checked view is another branch: the template taken before is refused on the re-check before signing
        let other = extend(&c, 4, h(0x52), 3);
        let after = verify_header_chain_v1(&t, &other, domain(), NOW + STEP, &lim()).unwrap();
        assert!(matches!(check_template_v1(&ok, &after), Err(VerifyErrorV1::StaleParent { .. })));
    }

    /// **L3 malicious fixtures**: a fake class row, a fake bond eligibility, a proof against a header off the verified chain — each refused
    /// before execution and before signing; and a correctly proven state on a non-canonical branch is never labelled VERIFIED_REMOTE.
    #[test]
    fn l3_refuses_fake_rows_and_off_chain_proofs_and_a_correct_proof_does_not_verify_the_fork_choice() {
        let (state, bond, class) = fixture_state(false);
        let root = state.state_root();
        let c = checkpoint_header(root);
        let chain = extend(&c, 3, root, 1);
        let t = trusted(&c, CheckpointTrustV1::Signed { issued_at_daa: 100 });
        let v = verify_header_chain_v1(&t, &chain, domain(), NOW, &lim()).unwrap();
        let at = chain.last().unwrap();
        let bonds = prove_bonds_v1(&state);
        let classes = prove_classes_v1(&state);
        let b = bond_on_chain_v1(&v, at, &bonds, &bond).unwrap();
        let k = class_on_chain_v1(&v, at, &classes, &class).unwrap();
        assert!(check_job_facts_v1(&job(bond, class), &k, &b).is_ok(), "the honest job checks out");
        // a fake class row: the node names another artifact root
        let fake_class = NodeJobFactsV1 { artifact_root: h(0xFA), ..job(bond, class) };
        assert!(matches!(check_job_facts_v1(&fake_class, &k, &b), Err(VerifyErrorV1::ContradictsProof { .. })));
        // a class the state does not hold
        let absent = class_on_chain_v1(&v, at, &classes, &h(0xC2)).unwrap();
        assert!(absent.value.is_none());
        assert!(check_job_facts_v1(&NodeJobFactsV1 { class_id: h(0xC2), ..job(bond, class) }, &absent, &b).is_err());
        // a fake bond eligibility: the node says our key is the bond's; the proof says otherwise
        assert!(check_job_facts_v1(&NodeJobFactsV1 { bond_pubkey: vec![9; 2592], ..job(bond, class) }, &k, &b).is_err());
        // a proof against a header that is not on the verified chain (an unlinked header carrying the same root)
        let stray = header(vec![h(0x99)], at.daa_score, at.blue_score, at.timestamp, root, 777);
        assert!(matches!(bond_on_chain_v1(&v, &stray, &bonds, &bond), Err(VerifyErrorV1::ProofHeaderOffChain(_))));
        // a proof over another state (rows forged to the node's taste) does not open under the verified header's root
        let (other_state, ..) = fixture_state(true);
        let mut forged_rows = prove_bonds_v1(&other_state);
        forged_rows.collection.rows.clear();
        assert!(matches!(bond_on_chain_v1(&v, at, &forged_rows, &bond), Err(VerifyErrorV1::Proof(_))));
        // The facts are correct and proven — but at a header PAST the trusted checkpoint: the state may be a non-canonical branch's.
        let l2 = l2_status_v1(&t, &v, Some(at.hash), &lim());
        assert!(matches!(l2, L2StatusV1::Unverified(_)));
        assert_eq!(mode_label_v1(false, true, true, &l2), ModeLabelV1::HeaderVerifiedForkChoiceUnverified);
        // Proven AT the trusted (signed) checkpoint, the chain not moved past it: L2 is established — VERIFIED_REMOTE.
        let at_c = verify_header_chain_v1(&t, &chain[..1], domain(), NOW - 9 * STEP, &lim()).unwrap();
        let l2c = l2_status_v1(&t, &at_c, Some(c.hash), &lim());
        assert_eq!(l2c, L2StatusV1::EstablishedAtTrustedCheckpoint);
        assert_eq!(mode_label_v1(false, true, true, &l2c), ModeLabelV1::VerifiedRemote);
        // …but a typed-in pin is an L1 anchor only, and a checkpoint the chain has moved past is no longer the decision point.
        let typed = trusted(&c, CheckpointTrustV1::UserPinned);
        assert!(matches!(l2_status_v1(&typed, &at_c, Some(c.hash), &lim()), L2StatusV1::Unverified(_)));
        assert!(matches!(l2_status_v1(&t, &v, Some(c.hash), &lim()), L2StatusV1::Unverified(_)));
    }

    /// **A restart, another peer** (the P2 correction's last attack test): verification is a pure function of the checkpoint the client
    /// holds and the bytes a peer serves now. A second peer serving the same DAG reaches the same verdict; a second peer serving a doctored
    /// copy is refused even though the first peer was honest — no trust is carried over.
    #[test]
    fn a_restarted_client_re_verifies_from_scratch_whoever_serves() {
        let (state, bond, _) = fixture_state(false);
        let root = state.state_root();
        let c = checkpoint_header(root);
        let t = trusted(&c, CheckpointTrustV1::Signed { issued_at_daa: 100 });
        let served = extend(&c, 3, root, 1);
        let first = verify_header_chain_v1(&t, &served, domain(), NOW, &lim()).unwrap();
        let second = verify_header_chain_v1(&t, &served.clone(), domain(), NOW, &lim()).unwrap();
        assert_eq!(first, second, "the same DAG from another peer: the same verdict");
        let bonds = prove_bonds_v1(&state);
        assert_eq!(
            bond_on_chain_v1(&first, served.last().unwrap(), &bonds, &bond).unwrap(),
            bond_on_chain_v1(&second, served.last().unwrap(), &bonds, &bond).unwrap()
        );
        let mut doctored = served.clone();
        doctored[1].blue_score += 5;
        assert!(verify_header_chain_v1(&t, &doctored, domain(), NOW, &lim()).is_err(), "the new peer's doctored copy is refused");
        // A stale signed checkpoint is refused by `checkpoint::verify_signed_checkpoint` (CheckpointError::Stale) before any of this runs;
        // a stale tip is refused here.
        assert!(matches!(
            verify_header_chain_v1(&t, &served, domain(), NOW + 2 * 3_600_000, &lim()),
            Err(VerifyErrorV1::StaleTip { .. })
        ));
    }
}
