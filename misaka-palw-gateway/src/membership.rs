//! **ADR-0095 §4.8 and §7 step 6 — the reference membership check: verify the signature, read the
//! tier, choose a queue.**
//!
//! A gateway that serves a line's holders ahead of strangers has three questions, and this module
//! answers each where the answer belongs:
//!
//! ```text
//!   who is asking?      signatures over a challenge THIS gateway issued — carrier lane: ML-DSA-87
//!                       under PALW_MODEL_BENEFIT_CHALLENGE_MLDSA87_CONTEXT, with the public key;
//!                       EVM lane: EIP-191 personal_sign, recovered by the EVM lane's own ecrecover
//!                       (alloy / k256) — each giving the holder id the fold keys that key's
//!                       positions by
//!   what do they hold?  getPalwModelBenefitTier(line, ids): the CHAIN's tier at the tip, never a
//!                       number this process derives from positions it read (ADR-0095 §4.3)
//!   what do they get?   PRIORITY_INFERENCE → the priority queue. Everything else a tier grants is
//!                       reported in the answer, and is the operator's to honour off chain
//! ```
//!
//! **Nothing here is consensus, and nothing is spent.** A proof is checked and forgotten. The only
//! state is the book of challenges this gateway issued, and it is what makes a proof single-use: a
//! proof copied off a log or a proxy answers a challenge that has already been consumed.
//!
//! **Refused by name, never downgraded.** A request whose proof does not verify is a 403 naming the
//! reason, not a job quietly run in the public queue: a member whose proof failed must learn why,
//! and a stranger replaying one must not be served as anything.

use std::collections::{HashMap, VecDeque};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use kaspa_consensus_core::palw_model_benefits_v1::{
    PALW_MODEL_BENEFIT_CHALLENGE_MLDSA87_CONTEXT, PALW_MODEL_BENEFIT_MAX_PROOF_IDS, grant, palw_model_benefit_challenge_v1,
};
use kaspa_consensus_core::palw_model_market_v1::palw_model_holder_of_pubkey_v1;
use kaspa_hashes::Hash64;
use serde::Deserialize;

/// How long a challenge may wait for its proof. Long enough for a person to approve a signature in
/// a wallet; short enough that the book of outstanding challenges stays small.
pub const CHALLENGE_TTL: Duration = Duration::from_secs(300);

/// The most challenges outstanding at once. Past it the OLDEST is dropped, so a flood of challenge
/// requests costs the flooder their own earlier challenges and never the gateway's memory.
pub const MAX_OUTSTANDING_CHALLENGES: usize = 4_096;

/// In-flight places kept for members whose tier grants `PRIORITY_INFERENCE`: a public queue that
/// fills to the brim must not turn a member away with the 503 a stranger gets.
pub const MEMBER_RESERVED_IN_FLIGHT: usize = 2;

/// A challenge nonce, in bytes.
pub const NONCE_LEN: usize = 32;

/// Which queue a job waits in for the one job slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Queue {
    Public,
    /// A member whose tier grants `PRIORITY_INFERENCE`.
    Priority,
}

impl Queue {
    pub fn name(self) -> &'static str {
        match self {
            Queue::Public => "public",
            Queue::Priority => "priority",
        }
    }
}

/// §4.2's one grant a SERVER acts on: the front of the queue. The other bits are reported.
pub fn queue_for(grants: u32) -> Queue {
    if grants & grant::PRIORITY_INFERENCE != 0 { Queue::Priority } else { Queue::Public }
}

/// How deep the in-flight queue may be for a job in `queue`. Without a membership line every job
/// has the whole queue, exactly as before this module existed; with one, the public queue stops
/// `MEMBER_RESERVED_IN_FLIGHT` short.
pub fn in_flight_cap(max_in_flight: usize, queue: Queue, membership_enabled: bool) -> usize {
    match (membership_enabled, queue) {
        (false, _) | (true, Queue::Priority) => max_in_flight,
        (true, Queue::Public) => max_in_flight.saturating_sub(MEMBER_RESERVED_IN_FLIGHT).max(1),
    }
}

/// The line whose holders this gateway serves ahead of strangers, and the network the proofs are
/// bound to (the identity's network domain — the same value every registry message carries).
#[derive(Clone, Debug)]
pub struct MembershipConfig {
    pub line_id: Hash64,
    pub network_domain: Hash64,
}

/// The request body's `misaka_membership`: one challenge, answered by every key the person holds
/// the line under. Unknown fields are refused, like every other field of this surface.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MembershipProof {
    /// Hex of the nonce `GET /v1/membership/challenge` issued.
    pub nonce: String,
    /// The DAA score the same challenge named.
    pub daa: u64,
    #[serde(default)]
    pub carrier: Vec<CarrierSignature>,
    #[serde(default)]
    pub evm: Vec<EvmSignature>,
}

/// A carrier-lane key's answer: the ML-DSA-87 public key (the holder id is derived from it, never
/// taken from the request) and its signature over the challenge.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CarrierSignature {
    pub public_key: String,
    pub signature: String,
}

/// An EVM account's answer: the account and its 65-byte `personal_sign` signature (`r ‖ s ‖ v`).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvmSignature {
    pub address: String,
    pub signature: String,
}

// ---- the challenge book ---------------------------------------------------------------------

#[derive(Default)]
struct ChallengeBook {
    issued: HashMap<[u8; NONCE_LEN], (u64, Instant)>,
    order: VecDeque<[u8; NONCE_LEN]>,
}

/// Every challenge this gateway has issued and not yet seen answered.
#[derive(Default)]
pub struct Challenges {
    book: Mutex<ChallengeBook>,
}

impl Challenges {
    /// A fresh challenge bound to `daa`.
    pub fn issue(&self, daa: u64) -> [u8; NONCE_LEN] {
        let mut nonce = [0u8; NONCE_LEN];
        rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut nonce);
        self.issue_with(nonce, daa, Instant::now());
        nonce
    }

    fn issue_with(&self, nonce: [u8; NONCE_LEN], daa: u64, now: Instant) {
        let mut book = self.book.lock().expect("the challenge lock is never poisoned");
        // Drop what can no longer be answered — consumed (absent from the map) or expired — from
        // the front, then enforce the bound on what is left.
        while let Some(front) = book.order.front().copied() {
            match book.issued.get(&front) {
                Some((_, at)) if now.duration_since(*at) < CHALLENGE_TTL => break,
                _ => {
                    book.order.pop_front();
                    book.issued.remove(&front);
                }
            }
        }
        while book.order.len() >= MAX_OUTSTANDING_CHALLENGES {
            if let Some(oldest) = book.order.pop_front() {
                book.issued.remove(&oldest);
            }
        }
        book.issued.insert(nonce, (daa, now));
        book.order.push_back(nonce);
    }

    /// Consume a challenge. It is removed whatever happens next: a proof answers ONE attempt, so a
    /// failed signature cannot be retried against the same challenge and a good one cannot be
    /// replayed.
    pub fn consume(&self, nonce: &[u8], daa: u64) -> Result<(), String> {
        self.consume_at(nonce, daa, Instant::now())
    }

    fn consume_at(&self, nonce: &[u8], daa: u64, now: Instant) -> Result<(), String> {
        let key: [u8; NONCE_LEN] =
            nonce.try_into().map_err(|_| format!("the nonce is {} bytes; this gateway issues {NONCE_LEN}", nonce.len()))?;
        let mut book = self.book.lock().expect("the challenge lock is never poisoned");
        let Some((issued_daa, at)) = book.issued.remove(&key) else {
            return Err("the nonce is not one this gateway issued, or it was already used — fetch a fresh challenge from \
                        GET /v1/membership/challenge"
                .to_string());
        };
        if now.duration_since(at) >= CHALLENGE_TTL {
            return Err(format!("the challenge expired ({} s)", CHALLENGE_TTL.as_secs()));
        }
        if issued_daa != daa {
            return Err(format!("the proof names DAA {daa}; the challenge was issued at DAA {issued_daa}"));
        }
        Ok(())
    }
}

/// `GET /v1/membership/challenge`'s answer: everything a holder's tool needs to build the message,
/// named so a client never has to guess an encoding.
pub fn challenge_json(cfg: &MembershipConfig, nonce: &[u8; NONCE_LEN], daa: u64) -> serde_json::Value {
    serde_json::json!({
        "line": cfg.line_id.to_string(),
        "network_domain": cfg.network_domain.to_string(),
        "nonce": faster_hex::hex_string(nonce),
        "daa": daa,
        "expires_in_secs": CHALLENGE_TTL.as_secs(),
        "message": "palw_model_benefit_challenge_v1(network_domain, line, holder, nonce, daa) — one message per holder id",
        "carrier_lane": {
            "holder": "palw_model_holder_of_pubkey_v1(public_key)",
            "sign": "ML-DSA-87 over the message's 64 bytes",
            "context": String::from_utf8_lossy(PALW_MODEL_BENEFIT_CHALLENGE_MLDSA87_CONTEXT),
            "cli": "misaka palw benefits --line <line> --key-file <seed> --nonce <nonce> --daa <daa>",
        },
        "evm_lane": {
            "holder": "evm_holder_v1(EVM_CHAIN_ID, address)",
            "sign": "personal_sign(0x<message hex>, address): EIP-191 over the message's 64 bytes",
        },
        "send": "the chat request's body carries {\"misaka_membership\": {\"nonce\", \"daa\", \"carrier\": [{\"public_key\", \"signature\"}], \"evm\": [{\"address\", \"signature\"}]}}",
    })
}

// ---- signatures → holder ids ----------------------------------------------------------------

fn decode_hex(text: &str, what: &str) -> Result<Vec<u8>, String> {
    let t = text.strip_prefix("0x").unwrap_or(text);
    let mut out = vec![0u8; t.len() / 2];
    if t.is_empty() || !t.len().is_multiple_of(2) || faster_hex::hex_decode(t.as_bytes(), &mut out).is_err() {
        return Err(format!("{what} is not hex"));
    }
    Ok(out)
}

/// Every signature in `proof`, checked against this gateway's challenge; the holder ids they prove,
/// sorted and each once. One bad signature refuses the whole proof — a person proves what they
/// hold, and a proof that is partly somebody else's is not theirs.
pub fn holders_from_proof(cfg: &MembershipConfig, nonce: &[u8], proof: &MembershipProof) -> Result<Vec<Hash64>, String> {
    let count = proof.carrier.len() + proof.evm.len();
    if count == 0 {
        return Err("the proof carries no signature".to_string());
    }
    if count > PALW_MODEL_BENEFIT_MAX_PROOF_IDS {
        return Err(format!("the proof carries {count} signatures; at most {PALW_MODEL_BENEFIT_MAX_PROOF_IDS} ids are summed"));
    }
    let mut ids = Vec::with_capacity(count);
    for c in &proof.carrier {
        ids.push(carrier_holder(cfg, nonce, proof.daa, c)?);
    }
    for e in &proof.evm {
        ids.push(evm_holder(cfg, nonce, proof.daa, e)?);
    }
    ids.sort();
    ids.dedup();
    Ok(ids)
}

/// A carrier-lane answer: the holder id is DERIVED from the public key, and the signature must
/// cover the challenge built for that id — so a key can only ever prove its own holding.
pub fn carrier_holder(cfg: &MembershipConfig, nonce: &[u8], daa: u64, sig: &CarrierSignature) -> Result<Hash64, String> {
    let public_key = decode_hex(&sig.public_key, "a carrier public_key")?;
    let signature = decode_hex(&sig.signature, "a carrier signature")?;
    if public_key.len() != kaspa_txscript::MLDSA87_PK_LEN {
        return Err(format!(
            "a carrier public_key is {} bytes; an ML-DSA-87 key is {}",
            public_key.len(),
            kaspa_txscript::MLDSA87_PK_LEN
        ));
    }
    let holder = palw_model_holder_of_pubkey_v1(&public_key);
    let message = palw_model_benefit_challenge_v1(cfg.network_domain, &cfg.line_id, &holder, nonce, daa);
    match kaspa_txscript::verify_mldsa87_with_context(
        &public_key,
        message.as_byte_slice(),
        &signature,
        PALW_MODEL_BENEFIT_CHALLENGE_MLDSA87_CONTEXT,
    ) {
        Ok(true) => Ok(holder),
        Ok(false) => Err(format!(
            "the carrier-lane signature for holder {holder} does not verify over this challenge (the line, the network, the \
             nonce and the height are all inside it)"
        )),
        Err(e) => Err(format!("a carrier-lane signature is malformed: {e}")),
    }
}

fn parse_evm_address(text: &str) -> Result<[u8; 20], String> {
    let raw = decode_hex(text, "an EVM address")?;
    raw.try_into().map_err(|_| format!("the EVM address '{text}' is not 20 bytes"))
}

/// An EVM account's answer: `personal_sign` over the challenge built for that account's holder id,
/// recovered — by `alloy-primitives` on k256, the ecrecover the EVM lane itself links through
/// `kaspa-evm` — to an account that must be the one named. No curve enters this binary for it:
/// k256 is already here, through the derivation step's EVM.
pub fn evm_holder(cfg: &MembershipConfig, nonce: &[u8], daa: u64, sig: &EvmSignature) -> Result<Hash64, String> {
    use alloy_primitives::{B256, PrimitiveSignature};
    use kaspa_consensus_core::evm::{EVM_CHAIN_ID, EvmAddress, model_market::evm_holder_v1};
    use kaspa_consensus_core::palw_model_benefits_v1::palw_model_benefit_challenge_evm_digest_v1;

    let address = parse_evm_address(&sig.address)?;
    let raw = decode_hex(&sig.signature, "an EVM signature")?;
    // 65 bytes, r ‖ s ‖ v, v in 27/28 or 0/1 — what `personal_sign` returns.
    let signature = PrimitiveSignature::from_raw(&raw).map_err(|e| format!("the EVM signature: {e}"))?;
    let holder = evm_holder_v1(EVM_CHAIN_ID, &EvmAddress::from_bytes(address));
    let challenge = palw_model_benefit_challenge_v1(cfg.network_domain, &cfg.line_id, &holder, nonce, daa);
    let digest = palw_model_benefit_challenge_evm_digest_v1(&challenge);
    let recovered = signature
        .recover_address_from_prehash(&B256::from(digest))
        .map_err(|_| format!("the EVM signature for {} recovers to no key", sig.address))?;
    if recovered.into_array() != address {
        return Err(format!(
            "the EVM signature was made by another account than {} over this challenge (the line, the network, the nonce \
             and the height are all inside it)",
            sig.address
        ));
    }
    Ok(holder)
}

// ---- the verdict ----------------------------------------------------------------------------

/// What the gateway decided and on what: reported in the answer's `misaka.membership`, so a member
/// can see which tier the chain gave them and which queue that bought.
#[derive(Clone, Debug)]
pub struct Verdict {
    pub holders: Vec<Hash64>,
    pub tip_daa: u64,
    pub units: u64,
    pub tenure_daa: u64,
    pub tier_index: Option<u32>,
    pub grants: u32,
    pub grant_names: Vec<String>,
    pub note: String,
    pub lapsed: Option<String>,
    pub queue: Queue,
}

impl Verdict {
    pub fn from_tier(holders: Vec<Hash64>, r: &kaspa_rpc_core::GetPalwModelBenefitTierResponse) -> Self {
        let grants = r.tier.as_ref().map(|t| t.grants).unwrap_or(0);
        Self {
            holders,
            tip_daa: r.tip_daa,
            units: r.units,
            tenure_daa: r.tenure_daa,
            tier_index: r.tier_index,
            grants,
            grant_names: r.tier.as_ref().map(|t| t.grant_names.clone()).unwrap_or_default(),
            note: r.tier.as_ref().map(|t| t.note.clone()).unwrap_or_default(),
            lapsed: r.benefits.as_ref().and_then(|b| b.lapsed.clone()),
            queue: queue_for(grants),
        }
    }

    pub fn json(&self) -> serde_json::Value {
        serde_json::json!({
            "verified": true,
            "holders": self.holders.iter().map(|h| h.to_string()).collect::<Vec<_>>(),
            "tip_daa": self.tip_daa,
            "units": self.units,
            "tenure_daa": self.tenure_daa,
            "tier_index": self.tier_index,
            "grant_bits": self.grants,
            "grants": self.grant_names,
            "note": self.note,
            "lapsed": self.lapsed,
            "queue": self.queue.name(),
            // The one grant this server acts on is the queue; the others are the operator's to
            // honour off chain, and saying so is part of not overclaiming.
            "acted_on": if self.queue == Queue::Priority { vec!["PRIORITY_INFERENCE"] } else { Vec::new() },
        })
    }
}

/// The whole check, in the order a failure should be reported: the nonce (consumed whatever happens
/// next), the signatures, then the chain's tier.
pub fn check(
    cfg: &MembershipConfig,
    challenges: &Challenges,
    proof: &MembershipProof,
    read_tier: impl FnOnce(&Hash64, &[Hash64]) -> Result<kaspa_rpc_core::GetPalwModelBenefitTierResponse, String>,
) -> Result<Verdict, String> {
    let nonce = decode_hex(&proof.nonce, "the nonce")?;
    challenges.consume(&nonce, proof.daa)?;
    let holders = holders_from_proof(cfg, &nonce, proof)?;
    let r = read_tier(&cfg.line_id, &holders).map_err(|e| format!("the node could not read the tier: {e}"))?;
    if !r.exists {
        return Err(format!("the chain holds no line {} — this gateway's --membership-line names nothing", cfg.line_id));
    }
    Ok(Verdict::from_tier(holders, &r))
}

// ---- the slot ---------------------------------------------------------------------------------

#[derive(Default)]
struct SlotState {
    busy: bool,
    priority_waiting: usize,
}

/// **The one job slot, with a front door for members.** The resident worker runs one job at a
/// time; this decides who runs next. A priority waiter goes ahead of every public one. Among
/// equals it is whoever the condition variable wakes — the fairness the bare worker mutex gave,
/// which is none — and the queue in front of it is bounded either way (`in_flight_cap`).
///
/// A member flood can hold the public queue back for as long as it lasts; it cannot grow past the
/// in-flight cap, and the public queue is still served the moment no member is waiting.
#[derive(Default)]
pub struct SlotGate {
    state: Mutex<SlotState>,
    turn: Condvar,
}

/// Holds the slot; dropping it (a finished job, an error, a panic unwinding) frees it.
pub struct SlotGuard<'a> {
    gate: &'a SlotGate,
}

impl SlotGate {
    pub fn acquire(&self, queue: Queue) -> SlotGuard<'_> {
        let mut s = self.state.lock().expect("the slot lock is never poisoned");
        if queue == Queue::Priority {
            s.priority_waiting += 1;
        }
        while s.busy || (queue == Queue::Public && s.priority_waiting > 0) {
            s = self.turn.wait(s).expect("the slot lock is never poisoned");
        }
        if queue == Queue::Priority {
            s.priority_waiting -= 1;
        }
        s.busy = true;
        SlotGuard { gate: self }
    }
}

impl Drop for SlotGuard<'_> {
    fn drop(&mut self) {
        let mut s = self.gate.state.lock().expect("the slot lock is never poisoned");
        s.busy = false;
        drop(s);
        self.gate.turn.notify_all();
    }
}

/// Where a gateway keeps both halves of the door: the challenges it issued and the slot.
#[derive(Default)]
pub struct MembershipDoor {
    pub challenges: Challenges,
    pub slot: SlotGate,
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_pq_validator_core::ValidatorKey;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn cfg() -> MembershipConfig {
        MembershipConfig { line_id: Hash64::from_slice(&[0x4c; 64]), network_domain: Hash64::from_slice(&[0x6e; 64]) }
    }

    fn key(seed: u8) -> ValidatorKey {
        ValidatorKey::from_seed([seed; kaspa_pq_validator_core::VALIDATOR_SEED_LEN])
    }

    /// What `misaka palw benefits --nonce --daa` signs, spelled independently of the verifier.
    fn carrier_answer(c: &MembershipConfig, k: &ValidatorKey, nonce: &[u8], daa: u64) -> CarrierSignature {
        let holder = palw_model_holder_of_pubkey_v1(k.public_key());
        let message = palw_model_benefit_challenge_v1(c.network_domain, &c.line_id, &holder, nonce, daa);
        let signature = k.sign_with_context(message.as_byte_slice(), PALW_MODEL_BENEFIT_CHALLENGE_MLDSA87_CONTEXT);
        CarrierSignature { public_key: faster_hex::hex_string(k.public_key()), signature: faster_hex::hex_string(&signature) }
    }

    fn tier_response(grants: u32, units: u64) -> kaspa_rpc_core::GetPalwModelBenefitTierResponse {
        kaspa_rpc_core::GetPalwModelBenefitTierResponse {
            exists: true,
            units,
            tip_daa: 900,
            tier_index: Some(0),
            tier: Some(kaspa_rpc_core::RpcPalwModelBenefitTier {
                min_units: 1,
                grants,
                grant_names: grant::names_of(grants).into_iter().map(String::from).collect(),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    /// §4.8, carrier lane: a key proves its OWN holding, over THIS challenge, and nothing else.
    #[test]
    fn a_carrier_key_proves_its_own_holding_over_this_challenge_only() {
        let c = cfg();
        let k = key(7);
        let nonce = [3u8; NONCE_LEN];
        let answer = carrier_answer(&c, &k, &nonce, 500);
        assert_eq!(carrier_holder(&c, &nonce, 500, &answer), Ok(palw_model_holder_of_pubkey_v1(k.public_key())));

        // The same signature is not a proof of anything else: another height, nonce, line or
        // network each change the message it would have to cover.
        assert!(carrier_holder(&c, &nonce, 501, &answer).is_err(), "another height");
        assert!(carrier_holder(&c, &[4u8; NONCE_LEN], 500, &answer).is_err(), "another nonce");
        let other_line = MembershipConfig { line_id: Hash64::from_slice(&[0x4d; 64]), ..c.clone() };
        assert!(carrier_holder(&other_line, &nonce, 500, &answer).is_err(), "another line");
        let other_net = MembershipConfig { network_domain: Hash64::from_slice(&[0x6f; 64]), ..c.clone() };
        assert!(carrier_holder(&other_net, &nonce, 500, &answer).is_err(), "another network");

        // Another person's public key with this signature: the id is derived from the key, so the
        // message the signature would have to cover changes with it.
        let stolen = CarrierSignature { public_key: faster_hex::hex_string(key(8).public_key()), ..answer.clone() };
        assert!(carrier_holder(&c, &nonce, 500, &stolen).is_err());

        // A signature under a DIFFERENT context — a sell's — is not a membership proof, even over
        // the same bytes. That is what the dedicated context buys.
        let holder = palw_model_holder_of_pubkey_v1(k.public_key());
        let message = palw_model_benefit_challenge_v1(c.network_domain, &c.line_id, &holder, &nonce, 500);
        let as_sell =
            k.sign_with_context(message.as_byte_slice(), kaspa_consensus_core::palw_model_market_v1::PALW_MODEL_SELL_MLDSA87_CONTEXT);
        let wrong_context = CarrierSignature { signature: faster_hex::hex_string(&as_sell), ..answer };
        assert!(carrier_holder(&c, &nonce, 500, &wrong_context).is_err());
    }

    /// §4.8: a challenge answers one attempt — issued here, unexpired, at its own height, once.
    #[test]
    fn a_challenge_answers_one_attempt() {
        let book = Challenges::default();
        let t0 = Instant::now();
        let n = [1u8; NONCE_LEN];
        book.issue_with(n, 700, t0);
        assert!(book.consume_at(&n, 701, t0).unwrap_err().contains("issued at DAA 700"), "the height is part of the challenge");
        // …and that failure spent it.
        assert!(book.consume_at(&n, 700, t0).unwrap_err().contains("already used"));

        let m = [2u8; NONCE_LEN];
        book.issue_with(m, 700, t0);
        assert_eq!(book.consume_at(&m, 700, t0), Ok(()));
        assert!(book.consume_at(&m, 700, t0).is_err(), "a replay finds nothing");

        let late = [3u8; NONCE_LEN];
        book.issue_with(late, 700, t0);
        assert!(book.consume_at(&late, 700, t0 + CHALLENGE_TTL).unwrap_err().contains("expired"));

        assert!(book.consume_at(&[9u8; NONCE_LEN], 700, t0).is_err(), "a nonce nobody issued");
        assert!(book.consume_at(&[9u8; 8], 700, t0).is_err(), "a nonce of the wrong size");
    }

    /// The book is bounded: a flood of challenge requests evicts the flooder's own oldest ones.
    #[test]
    fn the_challenge_book_is_bounded() {
        let book = Challenges::default();
        let t0 = Instant::now();
        let nonce = |i: usize| {
            let mut n = [0u8; NONCE_LEN];
            n[..8].copy_from_slice(&(i as u64).to_le_bytes());
            n
        };
        for i in 0..=MAX_OUTSTANDING_CHALLENGES {
            book.issue_with(nonce(i), 1, t0);
        }
        assert!(book.book.lock().unwrap().issued.len() <= MAX_OUTSTANDING_CHALLENGES);
        assert!(book.consume_at(&nonce(0), 1, t0).is_err(), "the oldest was evicted");
        assert_eq!(book.consume_at(&nonce(MAX_OUTSTANDING_CHALLENGES), 1, t0), Ok(()), "the newest stands");
    }

    /// The whole path: nonce, signatures, the chain's tier, the queue — and each failure by name.
    #[test]
    fn the_check_is_the_nonce_then_the_signatures_then_the_chain() {
        let c = cfg();
        let door = MembershipDoor::default();
        let k = key(11);
        let nonce = door.challenges.issue(640);
        let proof = MembershipProof {
            nonce: faster_hex::hex_string(&nonce),
            daa: 640,
            carrier: vec![carrier_answer(&c, &k, &nonce, 640)],
            evm: vec![],
        };

        let mut asked = None;
        let v = check(&c, &door.challenges, &proof, |line, ids| {
            asked = Some((*line, ids.to_vec()));
            Ok(tier_response(grant::PRIORITY_INFERENCE | grant::SUPPORT, 42))
        })
        .expect("a good proof");
        assert_eq!(
            asked,
            Some((c.line_id, vec![palw_model_holder_of_pubkey_v1(k.public_key())])),
            "the chain was asked about exactly the proved id"
        );
        assert_eq!(v.queue, Queue::Priority);
        assert_eq!(v.units, 42);
        assert_eq!(v.json()["acted_on"], serde_json::json!(["PRIORITY_INFERENCE"]));

        // The same proof again: the challenge is spent, and the chain is not even asked.
        let again = check(&c, &door.challenges, &proof, |_, _| panic!("a replay must not reach the node"));
        assert!(again.unwrap_err().contains("already used"));

        // A tier without the grant is a member in the public queue — recognised, reported, not
        // promoted.
        let n2 = door.challenges.issue(640);
        let p2 = MembershipProof {
            nonce: faster_hex::hex_string(&n2),
            daa: 640,
            carrier: vec![carrier_answer(&c, &k, &n2, 640)],
            evm: vec![],
        };
        let v2 = check(&c, &door.challenges, &p2, |_, _| Ok(tier_response(grant::EARLY_VERSION, 5))).unwrap();
        assert_eq!(v2.queue, Queue::Public);
        assert!(v2.json()["acted_on"].as_array().unwrap().is_empty());

        // No line on the chain: refused by name.
        let n3 = door.challenges.issue(640);
        let p3 = MembershipProof {
            nonce: faster_hex::hex_string(&n3),
            daa: 640,
            carrier: vec![carrier_answer(&c, &k, &n3, 640)],
            evm: vec![],
        };
        let none = check(&c, &door.challenges, &p3, |_, _| Ok(kaspa_rpc_core::GetPalwModelBenefitTierResponse::default()));
        assert!(none.unwrap_err().contains("holds no line"));

        // An empty proof and an over-long one.
        let n4 = door.challenges.issue(640);
        let empty = MembershipProof { nonce: faster_hex::hex_string(&n4), daa: 640, carrier: vec![], evm: vec![] };
        assert!(check(&c, &door.challenges, &empty, |_, _| unreachable!()).unwrap_err().contains("no signature"));
    }

    /// A body field this surface does not know is refused, not dropped — the doctrine of the rest
    /// of the request, applied to the proof.
    #[test]
    fn an_unknown_proof_field_is_refused() {
        let ok: Result<MembershipProof, _> = serde_json::from_str(r#"{"nonce":"00","daa":1,"carrier":[]}"#);
        assert!(ok.is_ok());
        let typo: Result<MembershipProof, _> = serde_json::from_str(r#"{"nonce":"00","daa":1,"carier":[]}"#);
        assert!(typo.is_err());
    }

    #[test]
    fn the_public_queue_stops_short_only_where_members_are_served() {
        assert_eq!(in_flight_cap(8, Queue::Public, false), 8, "no membership line: nothing changes");
        assert_eq!(in_flight_cap(8, Queue::Priority, true), 8);
        assert_eq!(in_flight_cap(8, Queue::Public, true), 8 - MEMBER_RESERVED_IN_FLIGHT);
        assert_eq!(in_flight_cap(1, Queue::Public, true), 1, "never zero");
    }

    /// The slot serves a waiting member before any waiting stranger.
    #[test]
    fn a_waiting_member_runs_before_a_waiting_stranger() {
        let gate = Arc::new(SlotGate::default());
        let order = Arc::new(Mutex::new(Vec::new()));
        let held = gate.acquire(Queue::Public); // the job running now
        let waiting = Arc::new(AtomicUsize::new(0));
        let mut threads = Vec::new();
        for (name, queue) in [("stranger", Queue::Public), ("member", Queue::Priority)] {
            let (gate, order, arrived) = (Arc::clone(&gate), Arc::clone(&order), Arc::clone(&waiting));
            threads.push(std::thread::spawn(move || {
                arrived.fetch_add(1, Ordering::SeqCst);
                let _slot = gate.acquire(queue);
                order.lock().unwrap().push(name);
            }));
            // Let the stranger register as a waiter first, so the member has to overtake it.
            while waiting.load(Ordering::SeqCst) < threads.len() {
                std::thread::yield_now();
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        drop(held);
        for t in threads {
            t.join().unwrap();
        }
        assert_eq!(*order.lock().unwrap(), vec!["member", "stranger"]);
    }

    /// §4.8, EVM lane: the digest a wallet's `personal_sign` hashes is the one consensus-core
    /// spells — checked against alloy's EIP-191, a second implementation this crate did not write.
    #[test]
    fn the_evm_digest_is_eip191_as_the_evm_lane_spells_it() {
        use kaspa_consensus_core::palw_model_benefits_v1::palw_model_benefit_challenge_evm_digest_v1;
        let c = cfg();
        let challenge = palw_model_benefit_challenge_v1(c.network_domain, &c.line_id, &Hash64::from_slice(&[1; 64]), &[2; 32], 3);
        assert_eq!(
            palw_model_benefit_challenge_evm_digest_v1(&challenge),
            alloy_primitives::eip191_hash_message(challenge.as_byte_slice()).0
        );
    }

    /// §4.8, EVM lane: `personal_sign` over the challenge recovers to the account, and to no other.
    #[test]
    fn an_evm_account_proves_its_own_holding_with_personal_sign() {
        use kaspa_consensus_core::evm::{EVM_CHAIN_ID, EvmAddress, model_market::evm_holder_v1};
        use kaspa_consensus_core::palw_model_benefits_v1::palw_model_benefit_challenge_evm_digest_v1;
        let c = cfg();
        let secret = k256::ecdsa::SigningKey::from_slice(&[0x42; 32]).unwrap();
        let public = secret.verifying_key().to_encoded_point(false);
        let address: [u8; 20] = alloy_primitives::keccak256(&public.as_bytes()[1..])[12..].try_into().unwrap();
        let holder = evm_holder_v1(EVM_CHAIN_ID, &EvmAddress::from_bytes(address));
        let nonce = [5u8; NONCE_LEN];
        let sign = |daa: u64| {
            let challenge = palw_model_benefit_challenge_v1(c.network_domain, &c.line_id, &holder, &nonce, daa);
            let digest = palw_model_benefit_challenge_evm_digest_v1(&challenge);
            let (signature, recovery) = secret.sign_prehash_recoverable(&digest).unwrap();
            let mut raw = signature.to_bytes().to_vec();
            raw.push(27 + recovery.to_byte()); // a wallet's v
            EvmSignature {
                address: format!("0x{}", faster_hex::hex_string(&address)),
                signature: format!("0x{}", faster_hex::hex_string(&raw)),
            }
        };
        let answer = sign(900);
        assert_eq!(evm_holder(&c, &nonce, 900, &answer), Ok(holder));
        assert!(evm_holder(&c, &nonce, 901, &answer).is_err(), "another height recovers another account");
        let claimed_other = EvmSignature { address: format!("0x{}", "ab".repeat(20)), ..answer.clone() };
        assert!(evm_holder(&c, &nonce, 900, &claimed_other).is_err(), "naming someone else's account proves nothing");
        // v as 0/1 is what some signers return; it is the same signature.
        let bytes = {
            let t = answer.signature.trim_start_matches("0x");
            let mut b = vec![0u8; t.len() / 2];
            faster_hex::hex_decode(t.as_bytes(), &mut b).unwrap();
            b[64] -= 27;
            b
        };
        let zero_one = EvmSignature { signature: faster_hex::hex_string(&bytes), ..answer };
        assert_eq!(evm_holder(&c, &nonce, 900, &zero_one), Ok(holder));
    }
}
