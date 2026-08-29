//! **The wire between a PALW pool and a miner that runs no node.**
//!
//! Line-delimited JSON, one message per line, in both directions. JSON because the thing being
//! carried is chain FACTS a miner must be able to read, print and argue with — a binary framing
//! would make "the pool told me my class target was X" unreadable at exactly the moment an
//! operator needs to read it.
//!
//! # What is being split, and where the cut falls
//!
//! `kaspad/src/palw_producer.rs` runs eight steps: facts, template, anchor, inference, nonce
//! grind, sign, retain-and-broadcast material, submit. Steps 1–3 and 7–8 need chain state and a
//! P2P mouth; steps 4–6 need only CPU and a key. **This protocol is that cut.** The pool keeps
//! what needs a node, the miner keeps what needs a secret, and neither can do the other's half.
//!
//! # Why every miner still needs its own bond
//!
//! Not policy — arithmetic. `base0_rc_job_anchor_v1` binds `(network domain, pre-pow, class,
//! BOND)`, so two miners under one bond at one template derive one anchor, run one identical
//! inference and search one identical space: the second miner is not additional work, it is a
//! duplicate. A bond per miner is what makes a second miner mean a second job. It is also what
//! keeps the accountability where the work is — a claim is slashed against the bond that signed
//! it, and a pool that signed for its miners would be a pool that could be slashed for them.
//!
//! # The one thing the miner must never be talked into signing
//!
//! An attempt signature and a pool-auth signature are both ML-DSA-87 over 64 bytes. If they
//! shared a signing CONTEXT, a hostile pool could hand a miner an `attempt_id_v2` as an "auth
//! challenge" and collect a signature over an execution the miner never ran — which is a
//! convictable claim carrying the miner's own bond. They do not share one:
//! [`PALW_POOL_AUTH_MLDSA87_CONTEXT`] is a different domain from
//! `PALW_ATTEMPT_V2_MLDSA87_CONTEXT`, so a signature made under one can never verify under the
//! other, whatever bytes were put in front of it.

use serde::{Deserialize, Serialize};

/// The protocol version a peer speaks. Bumped when a field's MEANING changes; additive fields do
/// not bump it, because `serde(default)` already makes those compatible in both directions.
pub const PALW_POOL_PROTOCOL_VERSION: u32 = 1;

/// **The signing domain for pool authentication, and it is not the attempt domain.** See the
/// module docs: this separation is what makes an auth challenge unable to be an attempt.
pub const PALW_POOL_AUTH_MLDSA87_CONTEXT: &[u8] = b"misaka-palw/pool-auth/mldsa87/v1";

/// The blake2b domain the auth message is keyed under.
pub const PALW_POOL_AUTH_DOMAIN: &[u8] = b"misaka-palw/pool/auth-message/v1";

/// A ceiling on one JSON line, so a peer cannot make the other allocate without bound. Material
/// for the floor's canonical job is kilobytes; a megabyte is generous and finite.
pub const PALW_POOL_MAX_LINE_BYTES: usize = 8 * 1024 * 1024;

/// **The message a miner signs to prove it holds the bond's key.**
///
/// Binds the pool's random session nonce (so a signature cannot be replayed onto another
/// session), the bond, the miner's own public key, the payout it asked to be paid at, and the
/// network — a signature made for one network's pool is not a signature for another's. Keyed
/// blake2b under [`PALW_POOL_AUTH_DOMAIN`], each field length-prefixed so no two different
/// tuples can collide into one message.
pub fn pool_auth_message_v1(
    session_nonce: &[u8; 32],
    network_id: &str,
    bond: &str,
    pubkey: &[u8],
    pay_address: &str,
) -> kaspa_hashes::Hash64 {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(PALW_POOL_AUTH_DOMAIN).to_state();
    for part in [&session_nonce[..], network_id.as_bytes(), bond.as_bytes(), pubkey, pay_address.as_bytes()] {
        state.update(&(part.len() as u64).to_le_bytes());
        state.update(part);
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    kaspa_hashes::Hash64::from_bytes(out)
}

/// Miner → pool.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MinerMessageV1 {
    /// The opening claim: who this miner says it is. Nothing is granted on it — the pool answers
    /// with a challenge and believes none of it until the signature comes back.
    Hello {
        protocol: u32,
        /// `<txid>:<index>` of the bond this miner produces under.
        bond: String,
        /// Hex ML-DSA-87 verification key. Checked against what the CHAIN says the bond
        /// registered, not against anything the miner asserts.
        pubkey: String,
        /// Where this miner wants its own blocks to pay. The pool builds the template around it,
        /// and the miner verifies it got what it asked for before spending an inference.
        pay_address: String,
        /// Free-form, for the pool's log. Never load-bearing.
        #[serde(default)]
        agent: String,
    },
    /// The answer to [`PoolMessageV1::Challenge`]: hex ML-DSA-87 signature over
    /// [`pool_auth_message_v1`], made under [`PALW_POOL_AUTH_MLDSA87_CONTEXT`].
    Auth { signature: String },
    /// Ready for work. Sent after the welcome and after each finished job.
    JobRequest {
        /// The last job this miner finished, if any — lets the pool retire its template.
        #[serde(default)]
        finished: Option<String>,
    },
    /// A won nonce, and everything the pool needs to mount it into the block it handed out.
    Solution(Box<SolutionV1>),
    /// The assigned range held no winner. Not a failure — it is the common case, and it is how
    /// the pool learns this miner is alive and wants the next range.
    Exhausted {
        job_id: String,
        /// How many nonces were actually tried, for the pool's share accounting.
        nonces_tried: u64,
    },
    /// Keeps a quiet connection open and carries the miner's own view of its rate.
    Heartbeat {
        #[serde(default)]
        nonces_tried: u64,
    },
}

/// What a miner sends when both lotteries came in under target.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SolutionV1 {
    pub job_id: String,
    pub nonce: u64,
    /// Borsh-encoded `PalwAttemptEnvelopeV2` (the attempt AND the miner's signature over its id),
    /// hex. Carried whole rather than field by field: the envelope's own encoding is what
    /// consensus reads, so re-spelling it here would be a second encoder to disagree with.
    pub envelope: String,
    /// The execution material behind the committed roots, hex. **This is the miner's half of the
    /// retention promise** — the attempt declares a `trace_retention_daa`, and a claim whose
    /// material nobody serves cannot be licensed. The miner has no node to serve it from, so it
    /// hands the bytes to the pool, which does.
    pub material: String,
}

/// Pool → miner.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PoolMessageV1 {
    /// The random the miner must sign. Sent to anything that says hello — the bond is checked
    /// against the chain first, so a challenge is already a statement that the bond is real.
    Challenge {
        protocol: u32,
        /// Hex, 32 bytes, fresh per connection.
        session_nonce: String,
        /// The network the pool is on, as consensus spells it (`testnet-11`). Part of the auth
        /// message, and what the miner checks Layer-0 against.
        network_id: String,
    },
    /// The bond is registered, the key matches it and the signature verified. Carries the class
    /// facts that do not move per template, so a job can stay small.
    Welcome {
        /// The class this pool produces for — the BASE-0 floor unless the operator says otherwise.
        class_id: String,
        /// What the chain says that class's weights are. The miner DERIVES the floor's artifact
        /// and refuses if its own derivation does not hash to this.
        artifact_root: String,
        /// Borsh hex of the network's `PalwCourtParamsV2`, which is what decides a class's
        /// registered geometry and therefore its id. Sent rather than assumed: a miner that
        /// reconstructed a default court would resolve a class the chain never registered.
        court: String,
        /// True when this class is the liveness floor — which is also the answer to "do I need a
        /// model file?", because the floor's weights are derived from a pinned seed.
        is_base_class: bool,
    },
    /// One template, one job. Everything a miner needs to run the inference, grind, sign — and to
    /// CHECK that what it was handed pays it.
    Job(Box<JobV1>),
    /// The solution landed (or did not).
    SolutionResult {
        job_id: String,
        accepted: bool,
        /// The block hash, when accepted.
        #[serde(default)]
        block_hash: String,
        /// Why not, when refused. Verbatim from the chain where the chain is what refused.
        #[serde(default)]
        reason: String,
    },
    /// The connection is over, and why. Sent instead of a silent close so an operator reading a
    /// miner's log learns whether the bond was the problem.
    Rejected { reason: String },
    /// The pool has nothing to hand out right now (unsynced, no template, budget spent). The
    /// miner waits and asks again rather than treating it as an error.
    Standby { reason: String, retry_after_ms: u64 },
}

/// The per-template work order.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct JobV1 {
    pub job_id: String,
    /// Borsh hex of the template's `Header`. The miner recomputes `pre_pow_hash_64` from it —
    /// the anchor is a function of that hash, so a miner that took the pool's word for it would
    /// be running a job it cannot check.
    pub header: String,
    /// Borsh hex of every transaction in the template, coinbase first. Present so the miner can
    /// recompute `hash_merkle_root` and confirm the coinbase the header commits to is the one
    /// that pays IT. Without this the payout is the pool's word; with it, it is arithmetic.
    pub transactions: Vec<String>,
    /// The chain facts admission will check the attempt against, at this template's chain point.
    pub class_id: String,
    pub artifact_root: String,
    /// Decimal `u128` — the class lottery's ceiling.
    pub class_target: String,
    /// Admission item 6 is an EQUALITY, so this is the one legal value, not a suggestion.
    pub pwu: u64,
    pub operator_id: String,
    /// Absolute DAA score the attempt must promise to retain its trace until.
    pub trace_retention_daa: u64,
    /// The half-open nonce range this miner owns for this job.
    pub nonce_start: u64,
    pub nonce_end: u64,
}

/// Read one line and parse it, refusing a line that is too long before it is allocated.
pub fn parse_line<T: for<'de> Deserialize<'de>>(line: &str) -> Result<T, String> {
    if line.len() > PALW_POOL_MAX_LINE_BYTES {
        return Err(format!("a {} byte line is past the {PALW_POOL_MAX_LINE_BYTES} ceiling", line.len()));
    }
    serde_json::from_str(line).map_err(|e| format!("not a message this version reads: {e}"))
}

/// Render one message as the single line it travels as.
pub fn encode_line<T: Serialize>(message: &T) -> Result<String, String> {
    let mut line = serde_json::to_string(message).map_err(|e| format!("could not encode: {e}"))?;
    if line.len() > PALW_POOL_MAX_LINE_BYTES {
        return Err(format!("a {} byte line is past the {PALW_POOL_MAX_LINE_BYTES} ceiling", line.len()));
    }
    line.push('\n');
    Ok(line)
}

pub fn to_hex(bytes: &[u8]) -> String {
    faster_hex::hex_string(bytes)
}

pub fn from_hex(s: &str) -> Result<Vec<u8>, String> {
    let mut out = vec![0u8; s.len() / 2];
    faster_hex::hex_decode(s.as_bytes(), &mut out).map_err(|e| format!("not hex: {e}"))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every message round-trips through the line codec it actually travels on.
    #[test]
    fn the_messages_round_trip_as_lines() {
        let hello = MinerMessageV1::Hello {
            protocol: PALW_POOL_PROTOCOL_VERSION,
            bond: "aa:0".into(),
            pubkey: "beef".into(),
            pay_address: "misakatest:qq".into(),
            agent: "miner/1".into(),
        };
        let line = encode_line(&hello).expect("encodes");
        assert!(line.ends_with('\n'), "a line message ends its line");
        assert_eq!(parse_line::<MinerMessageV1>(line.trim()).expect("parses"), hello);

        let job = PoolMessageV1::Job(Box::new(JobV1 {
            job_id: "j1".into(),
            header: "00".into(),
            transactions: vec!["01".into()],
            class_id: "c".into(),
            artifact_root: "r".into(),
            class_target: u128::MAX.to_string(),
            pwu: 7900,
            operator_id: "o".into(),
            trace_retention_daa: 1234,
            nonce_start: 0,
            nonce_end: 4_000_000,
        }));
        let line = encode_line(&job).expect("encodes");
        assert_eq!(parse_line::<PoolMessageV1>(line.trim()).expect("parses"), job);
        // u128 survives as a decimal string: JSON numbers would silently lose the top bits.
        assert!(line.contains(&u128::MAX.to_string()));
    }

    /// A missing optional field is a default, not a parse error — that is what lets a pool add a
    /// field without every miner in the field going dark.
    #[test]
    fn an_older_peers_message_still_parses() {
        let hello: MinerMessageV1 =
            parse_line(r#"{"type":"hello","protocol":1,"bond":"aa:0","pubkey":"be","pay_address":"m:q"}"#).expect("no agent");
        assert!(matches!(hello, MinerMessageV1::Hello { agent, .. } if agent.is_empty()));
        let req: MinerMessageV1 = parse_line(r#"{"type":"job_request"}"#).expect("no finished");
        assert_eq!(req, MinerMessageV1::JobRequest { finished: None });
    }

    /// **The auth message binds every field it names.** Change any one of them and the bytes a
    /// miner signs change — which is what stops a signature made for one session, one bond, one
    /// payout or one network being replayed onto another.
    #[test]
    fn the_auth_message_binds_the_whole_tuple() {
        let base = pool_auth_message_v1(&[7u8; 32], "testnet-11", "aa:0", b"key", "misakatest:qq");
        assert_ne!(base, pool_auth_message_v1(&[8u8; 32], "testnet-11", "aa:0", b"key", "misakatest:qq"));
        assert_ne!(base, pool_auth_message_v1(&[7u8; 32], "testnet-10", "aa:0", b"key", "misakatest:qq"));
        assert_ne!(base, pool_auth_message_v1(&[7u8; 32], "testnet-11", "aa:1", b"key", "misakatest:qq"));
        assert_ne!(base, pool_auth_message_v1(&[7u8; 32], "testnet-11", "aa:0", b"kex", "misakatest:qq"));
        assert_ne!(base, pool_auth_message_v1(&[7u8; 32], "testnet-11", "aa:0", b"key", "misakatest:qr"));
        assert_eq!(base, pool_auth_message_v1(&[7u8; 32], "testnet-11", "aa:0", b"key", "misakatest:qq"), "and it is a function");
        // Length-prefixed, so moving a boundary between two adjacent fields is a different message.
        assert_ne!(
            pool_auth_message_v1(&[7u8; 32], "testnet-1", "1aa:0", b"key", "misakatest:qq"),
            pool_auth_message_v1(&[7u8; 32], "testnet-11", "aa:0", b"key", "misakatest:qq")
        );
    }

    /// The auth domain is not the attempt domain. If this ever became an equality, a pool could
    /// collect attempt signatures by calling them challenges.
    #[test]
    fn the_auth_context_is_not_the_attempt_context() {
        assert_ne!(PALW_POOL_AUTH_MLDSA87_CONTEXT, kaspa_consensus_core::palw_attempt_v2::PALW_ATTEMPT_V2_MLDSA87_CONTEXT);
    }

    /// A line past the ceiling is refused rather than allocated.
    #[test]
    fn an_oversized_line_is_refused() {
        let huge = "x".repeat(PALW_POOL_MAX_LINE_BYTES + 1);
        assert!(parse_line::<MinerMessageV1>(&huge).is_err());
    }
}
