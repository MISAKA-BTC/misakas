//! **The miner's half: what a machine with no chain can still do for itself.**
//!
//! A pooled miner receives facts it cannot check and a template it can. This module is written
//! around that line. Everything checkable is checked before an inference is spent
//! ([`verify_job_pays_me`]), and what is left — the class target, the retention window — is
//! trusted the way any pooled miner trusts a pool, which is to say: not with its key, and not with
//! its payout.
//!
//! # The work, in the order it happens
//!
//! 1. **Check the job pays me.** Recompute `hash_merkle_root` over the transactions the pool sent
//!    and compare it to the header's; then read the coinbase and confirm an output pays this
//!    miner's script. Both halves are needed — the merkle root without the coinbase says nothing
//!    about who is paid, and the coinbase without the merkle root is a slip of paper the header
//!    never committed to.
//! 2. **Derive the anchor** from `pre_pow_hash_64` of that header — computed here, never taken
//!    from the wire, because the anchor is what the whole execution hangs from.
//! 3. **Run the inference.** For the floor this needs no file: the class's weights are derived
//!    from a pinned seed, which is what makes a laptop with nothing on it a viable miner.
//! 4. **Grind the assigned nonce range** against the class ticket and then Layer-0.
//! 5. **Sign once, on a hit**, under the ATTEMPT context — the only place this miner's key is ever
//!    used on an attempt, and a place the pool cannot reach.

use kaspa_consensus_core::header::Header;
use kaspa_consensus_core::palw_attempt_v2::{
    PALW_ATTEMPT_V2_MLDSA87_CONTEXT, PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, attempt_id_v2,
    challenge_v2, class_ticket_v2,
};
use kaspa_consensus_core::tx::{Transaction, TransactionOutpoint};
use kaspa_hashes::Hash64;

/// A job as the miner reads it, after the wire's hex has been turned back into things.
pub struct DecodedJobV1 {
    pub job_id: String,
    pub header: Header,
    pub transactions: Vec<Transaction>,
    pub class_id: Hash64,
    pub artifact_root: Hash64,
    pub class_target: u128,
    pub pwu: u64,
    pub operator_id: Hash64,
    pub trace_retention_daa: u64,
    pub nonce_start: u64,
    pub nonce_end: u64,
}

/// Why a job was refused before any work was spent on it.
#[derive(Debug, PartialEq, Eq)]
pub enum JobRefusalV1 {
    /// The transactions the pool sent do not hash to the merkle root the header commits to — so
    /// the coinbase shown is not the coinbase this block contains.
    MerkleMismatch { header_says: String, transactions_say: String },
    /// The block carries no transactions at all, so there is no coinbase to be paid by.
    NoCoinbase,
    /// The coinbase pays somebody, and it is not this miner.
    NotPaidHere,
    /// The template is not a PALW ConsensusV2 lane at all.
    WrongLane { algo: u8 },
    /// An empty or backwards nonce range.
    EmptyRange { start: u64, end: u64 },
}

impl std::fmt::Display for JobRefusalV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MerkleMismatch { header_says, transactions_say } => write!(
                f,
                "the pool's transactions hash to {transactions_say} and its header commits to {header_says} — the block it \
                 showed is not the block it would submit"
            ),
            Self::NoCoinbase => write!(f, "the template carries no transactions, so nothing in it pays anybody"),
            Self::NotPaidHere => {
                write!(f, "the coinbase of this template pays another script — this pool is not building blocks that pay this miner")
            }
            Self::WrongLane { algo } => write!(f, "the template declares pow algo {algo}, which is not the PALW ConsensusV2 lane"),
            Self::EmptyRange { start, end } => write!(f, "the assigned nonce range [{start}, {end}) is empty"),
        }
    }
}

/// **Check, before spending an inference, that this job is the job it claims to be.**
///
/// The two checks are one argument: the merkle root proves the transaction list is the one the
/// header commits to, and the coinbase scan proves that list pays this miner. Either alone proves
/// nothing — which is why a pool protocol that ships only a coinbase, or only a pre-pow hash,
/// leaves the payout as the pool's word.
pub fn verify_job_pays_me(job: &DecodedJobV1, my_script: &kaspa_consensus_core::tx::ScriptPublicKey) -> Result<(), JobRefusalV1> {
    if job.header.pow_algo_id != kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_COMMITTED_V2 {
        return Err(JobRefusalV1::WrongLane { algo: job.header.pow_algo_id });
    }
    if job.nonce_end <= job.nonce_start {
        return Err(JobRefusalV1::EmptyRange { start: job.nonce_start, end: job.nonce_end });
    }
    let coinbase = job.transactions.first().ok_or(JobRefusalV1::NoCoinbase)?;
    let computed = kaspa_consensus_core::merkle::calc_hash_merkle_root(job.transactions.iter());
    if computed != job.header.hash_merkle_root {
        return Err(JobRefusalV1::MerkleMismatch {
            header_says: job.header.hash_merkle_root.to_string(),
            transactions_say: computed.to_string(),
        });
    }
    if !coinbase.outputs.iter().any(|o| &o.script_public_key == my_script) {
        return Err(JobRefusalV1::NotPaidHere);
    }
    Ok(())
}

/// What the grind produced.
pub struct WonNonceV1 {
    pub nonce: u64,
    pub envelope: Vec<u8>,
    pub nonces_tried: u64,
}

/// The attempt every nonce in a job shares — every field but the challenge, which the nonce moves.
///
/// Built from the chain facts the job carries and the roots the execution produced. A miner cannot
/// choose any of them: five are the pool's relay of chain state and four are what its own
/// inference committed to.
#[allow(clippy::too_many_arguments)]
pub fn attempt_for_job_v1(
    network_domain: Hash64,
    job: &DecodedJobV1,
    bond: TransactionOutpoint,
    pubkey: Vec<u8>,
    outcome: &kaspa_consensus_core::palw_backend::PalwExecutionOutcomeV1,
) -> PalwAttemptUnsignedV2 {
    PalwAttemptUnsignedV2 {
        version: PALW_ATTEMPT_V2_VERSION,
        network_domain,
        challenge: Hash64::default(),
        class_id: job.class_id,
        executor_bond: bond,
        executor_pubkey: pubkey,
        operator_id: job.operator_id,
        artifact_root: job.artifact_root,
        trace_root: outcome.trace_root,
        output_root: outcome.output_root,
        execution_root: outcome.execution_root,
        pwu: job.pwu,
        trace_manifest_root: outcome.trace_manifest_root,
        trace_chunk_count: outcome.trace_chunk_count,
        trace_retention_daa: job.trace_retention_daa,
    }
}

/// **Grind the assigned range, and sign only if it wins.**
///
/// The two lotteries in the order the producer runs them: the class ticket first (a hash), Layer-0
/// second (the expensive check), so the expensive one runs on the few nonces that got past the
/// cheap one. The signature is made ONCE, after both — it is outside `commitment_root_v2`, so
/// signing per nonce would throw away an ML-DSA-87 operation on every try.
///
/// `sig_len` is a real signature's length, used to pad the envelope during the search so the shape
/// gate sees the wire size the submitted block will have.
pub fn grind_and_sign_v1(
    job: &DecodedJobV1,
    mut attempt: PalwAttemptUnsignedV2,
    network_domain: Hash64,
    bond: TransactionOutpoint,
    network_id: &str,
    signing_key: &libcrux_ml_dsa::ml_dsa_87::MLDSA87SigningKey,
    sig_len: usize,
    should_stop: &dyn Fn() -> bool,
) -> Result<Option<WonNonceV1>, String> {
    let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(&job.header);
    let timestamp = job.header.timestamp;
    let mut header = job.header.clone();
    for nonce in job.nonce_start..job.nonce_end {
        // How many this call has tried, derived from where in the range it is rather than counted
        // beside it — one number, so the two cannot drift.
        let tried = nonce - job.nonce_start;
        // Checked on a coarse stride: a template goes stale as the past-median time moves, and a
        // miner that ground a whole range against a dead template would submit a solution the pool
        // could only throw away.
        if tried.is_multiple_of(4096) && should_stop() {
            break;
        }
        attempt.challenge = challenge_v2(network_domain, pre_pow, timestamp, nonce, job.class_id, &bond);
        if class_ticket_v2(&attempt) > job.class_target {
            continue;
        }
        header.nonce = nonce;
        header.palw_commitment = PalwAttemptEnvelopeV2 { attempt: attempt.clone(), signature: vec![0u8; sig_len] }.encode_wire();
        let state = kaspa_pow::StateLayer0::new(&header, network_id.as_bytes());
        if !state.check_pow_layer0(nonce).map(|(ok, _)| ok).unwrap_or(false) {
            continue;
        }
        let signature = libcrux_ml_dsa::ml_dsa_87::sign(
            signing_key,
            attempt_id_v2(&attempt).as_byte_slice(),
            PALW_ATTEMPT_V2_MLDSA87_CONTEXT,
            [0x5Au8; 32],
        )
        .map_err(|e| format!("ML-DSA-87 sign: {e:?}"))?
        .as_ref()
        .to_vec();
        let envelope = PalwAttemptEnvelopeV2 { attempt, signature }.encode_wire();
        return Ok(Some(WonNonceV1 { nonce, envelope, nonces_tried: tried + 1 }));
    }
    Ok(None)
}

/// The length of a real ML-DSA-87 signature from this key, for the search's padding.
pub fn signature_length_v1(signing_key: &libcrux_ml_dsa::ml_dsa_87::MLDSA87SigningKey) -> usize {
    libcrux_ml_dsa::ml_dsa_87::sign(signing_key, &[0u8; 64], PALW_ATTEMPT_V2_MLDSA87_CONTEXT, [0u8; 32])
        .map(|s| s.as_ref().len())
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------------------------
// The client: hello, prove the bond, then loop on jobs
// ---------------------------------------------------------------------------------------------

/// Everything the miner binary was told, once.
pub struct MinerConfigV1 {
    pub pool: String,
    pub bond_text: String,
    pub pay_address: String,
    pub agent: String,
    /// How long to wait before reconnecting after the pool hangs up or the socket dies.
    pub reconnect_after: std::time::Duration,
}

/// What the pool said in its welcome: the class this miner is about to resolve locally.
pub struct WelcomeV1 {
    pub class_id: Hash64,
    pub artifact_root: Hash64,
    pub court: kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2,
    pub is_base_class: bool,
}

/// Decode a job off the wire. Every hex field becomes the thing it stands for here, so the work
/// loop never handles a string it has to trust.
pub fn decode_job_v1(job: &crate::protocol::JobV1) -> Result<DecodedJobV1, String> {
    let header: Header = borsh::from_slice(&crate::protocol::from_hex(&job.header)?).map_err(|e| format!("header: {e}"))?;
    let transactions = job
        .transactions
        .iter()
        .map(|t| {
            crate::protocol::from_hex(t).and_then(|b| borsh::from_slice::<Transaction>(&b).map_err(|e| format!("transaction: {e}")))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let parse64 = |s: &str, what: &str| -> Result<Hash64, String> { s.parse::<Hash64>().map_err(|e| format!("{what}: {e}")) };
    Ok(DecodedJobV1 {
        job_id: job.job_id.clone(),
        header,
        transactions,
        class_id: parse64(&job.class_id, "class_id")?,
        artifact_root: parse64(&job.artifact_root, "artifact_root")?,
        class_target: job.class_target.parse::<u128>().map_err(|e| format!("class_target: {e}"))?,
        pwu: job.pwu,
        operator_id: parse64(&job.operator_id, "operator_id")?,
        trace_retention_daa: job.trace_retention_daa,
        nonce_start: job.nonce_start,
        nonce_end: job.nonce_end,
    })
}

/// **Resolve the class the pool named, locally, and refuse if this machine's answer differs.**
///
/// The floor derives from a pinned seed, so this is the step that needs no download — and it is
/// also the step that makes the pool unable to substitute a class. `resolve_class_v1` refuses
/// unless BOTH the id and the artifact root match what was asked for, so a pool that named a
/// different class gets a miner that stops rather than one that mines the wrong thing.
pub fn resolve_class_v1(welcome: &WelcomeV1) -> Result<misaka_palw_base0::backend::Base0Backend, String> {
    let resolved = misaka_palw_base0::classes::resolve_class_v1(&welcome.court, welcome.class_id, welcome.artifact_root, &[])
        .map_err(|e| format!("this miner cannot resolve the class the pool named: {e}"))?;
    Ok(misaka_palw_base0::backend::Base0Backend::new(resolved))
}

/// **One job, start to finish**, on the calling thread: check it pays this miner, derive the
/// anchor from the header, run the inference, grind, sign.
///
/// Returns `Ok(None)` when the range held no winner — the common case, and not an error.
#[allow(clippy::too_many_arguments)]
pub fn work_one_job_v1(
    job: &DecodedJobV1,
    backend: &dyn kaspa_consensus_core::palw_backend::PalwExecutionBackendV1,
    my_script: &kaspa_consensus_core::tx::ScriptPublicKey,
    network_domain: Hash64,
    network_id: &str,
    bond: TransactionOutpoint,
    keypair: &libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair,
    should_stop: &dyn Fn() -> bool,
) -> Result<Option<(WonNonceV1, Vec<u8>)>, String> {
    verify_job_pays_me(job, my_script).map_err(|e| e.to_string())?;
    // The anchor is computed from the header this miner holds — never taken from the wire.
    let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(&job.header);
    let anchor = misaka_palw_base0::produce::base0_rc_job_anchor_v1(network_domain, pre_pow, job.class_id, &bond);
    let (context, prompt) = backend.job_for_anchor(anchor).map_err(|e| format!("the job this template implies: {e}"))?;
    let outcome = backend.execute(&context, &prompt).map_err(|e| format!("the inference: {e}"))?;
    let material = outcome.material.clone();
    let attempt = attempt_for_job_v1(network_domain, job, bond, keypair.verification_key.as_ref().to_vec(), &outcome);
    let sig_len = signature_length_v1(&keypair.signing_key);
    let won = grind_and_sign_v1(job, attempt, network_domain, bond, network_id, &keypair.signing_key, sig_len, should_stop)?;
    Ok(won.map(|w| (w, material)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::subnets::SUBNETWORK_ID_COINBASE;
    use kaspa_consensus_core::tx::{ScriptPublicKey, ScriptVec, TransactionOutput};

    fn script(byte: u8) -> ScriptPublicKey {
        ScriptPublicKey::new(0, ScriptVec::from_slice(&[byte; 34]))
    }

    fn coinbase_paying(script: ScriptPublicKey) -> Transaction {
        Transaction::new(0, Vec::new(), vec![TransactionOutput::new(5000, script)], 0, SUBNETWORK_ID_COINBASE, 0, Vec::new())
    }

    fn job_paying(script: ScriptPublicKey) -> DecodedJobV1 {
        let transactions = vec![coinbase_paying(script)];
        let mut header = Header::from_precomputed_hash(Hash64::from_u64_word(1), Vec::new());
        header.pow_algo_id = kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_COMMITTED_V2;
        header.hash_merkle_root = kaspa_consensus_core::merkle::calc_hash_merkle_root(transactions.iter());
        DecodedJobV1 {
            job_id: "j".into(),
            header,
            transactions,
            class_id: Hash64::from_u64_word(1),
            artifact_root: Hash64::from_u64_word(2),
            class_target: u128::MAX,
            pwu: 7900,
            operator_id: Hash64::from_u64_word(3),
            trace_retention_daa: 100,
            nonce_start: 0,
            nonce_end: 10,
        }
    }

    /// A template that pays this miner, and whose header commits to the very list that says so.
    #[test]
    fn a_job_that_pays_this_miner_is_accepted() {
        let mine = script(0xAA);
        assert_eq!(verify_job_pays_me(&job_paying(mine.clone()), &mine), Ok(()));
    }

    /// **A pool that pays itself is caught before an inference is spent.**
    #[test]
    fn a_job_that_pays_somebody_else_is_refused() {
        let job = job_paying(script(0xBB));
        assert_eq!(verify_job_pays_me(&job, &script(0xAA)), Err(JobRefusalV1::NotPaidHere));
    }

    /// **And a pool that shows one coinbase while committing to another is caught too** — which is
    /// the attack the merkle check exists for. Without it, "the coinbase pays you" is a sentence
    /// the pool can simply say.
    #[test]
    fn a_coinbase_the_header_never_committed_to_is_refused() {
        let mut job = job_paying(script(0xAA));
        // The header keeps its original merkle root; the list is swapped for one that pays me.
        job.transactions = vec![coinbase_paying(script(0xAA)), coinbase_paying(script(0xCC))];
        assert!(matches!(verify_job_pays_me(&job, &script(0xAA)), Err(JobRefusalV1::MerkleMismatch { .. })));
    }

    /// The cheap structural refusals, each named.
    #[test]
    fn a_malformed_job_is_refused_by_name() {
        let mine = script(0xAA);

        let mut wrong_lane = job_paying(mine.clone());
        wrong_lane.header.pow_algo_id = 0;
        assert_eq!(verify_job_pays_me(&wrong_lane, &mine), Err(JobRefusalV1::WrongLane { algo: 0 }));

        let mut empty_range = job_paying(mine.clone());
        empty_range.nonce_end = empty_range.nonce_start;
        assert!(matches!(verify_job_pays_me(&empty_range, &mine), Err(JobRefusalV1::EmptyRange { .. })));

        let mut no_txs = job_paying(mine.clone());
        no_txs.transactions.clear();
        assert_eq!(verify_job_pays_me(&no_txs, &mine), Err(JobRefusalV1::NoCoinbase));
    }

    /// The grind honours its range and its stop signal, and it signs nothing when it loses.
    #[test]
    fn a_losing_range_signs_nothing_and_stops_when_told() {
        let kp = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([0x31u8; 32]);
        let mut job = job_paying(script(0xAA));
        // A target of zero: no ticket can be under it, so every nonce is a loss.
        job.class_target = 0;
        job.nonce_end = 64;
        let bond = TransactionOutpoint::new(kaspa_consensus_core::tx::TransactionId::from_u64_word(9), 0);
        let outcome = kaspa_consensus_core::palw_backend::PalwExecutionOutcomeV1 {
            trace_root: Hash64::from_u64_word(4),
            output_root: Hash64::from_u64_word(5),
            execution_root: Hash64::from_u64_word(6),
            trace_manifest_root: Hash64::from_u64_word(7),
            trace_chunk_count: 1,
            material: vec![1, 2, 3],
        };
        let domain = Hash64::from_u64_word(0xD0);
        let attempt = attempt_for_job_v1(domain, &job, bond, kp.verification_key.as_ref().to_vec(), &outcome);

        let won = grind_and_sign_v1(&job, attempt.clone(), domain, bond, "testnet-11", &kp.signing_key, 4627, &|| false)
            .expect("the grind runs");
        assert!(won.is_none(), "an unreachable target wins nothing");

        // And a stop signal ends it rather than running the range out.
        let stopped =
            grind_and_sign_v1(&job, attempt, domain, bond, "testnet-11", &kp.signing_key, 4627, &|| true).expect("the grind runs");
        assert!(stopped.is_none());
    }

    /// The attempt a job implies takes its chain facts from the job and its roots from the
    /// execution — a miner picks none of them.
    #[test]
    fn the_attempt_is_the_jobs_facts_and_the_executions_roots() {
        let job = job_paying(script(0xAA));
        let outcome = kaspa_consensus_core::palw_backend::PalwExecutionOutcomeV1 {
            trace_root: Hash64::from_u64_word(4),
            output_root: Hash64::from_u64_word(5),
            execution_root: Hash64::from_u64_word(6),
            trace_manifest_root: Hash64::from_u64_word(7),
            trace_chunk_count: 2,
            material: Vec::new(),
        };
        let bond = TransactionOutpoint::new(kaspa_consensus_core::tx::TransactionId::from_u64_word(9), 3);
        let attempt = attempt_for_job_v1(Hash64::from_u64_word(0xD0), &job, bond, vec![7u8; 4], &outcome);
        assert_eq!(attempt.class_id, job.class_id);
        assert_eq!(attempt.artifact_root, job.artifact_root);
        assert_eq!(attempt.pwu, job.pwu);
        assert_eq!(attempt.operator_id, job.operator_id);
        assert_eq!(attempt.trace_retention_daa, job.trace_retention_daa);
        assert_eq!(attempt.executor_bond, bond);
        assert_eq!(attempt.trace_root, outcome.trace_root);
        assert_eq!(attempt.execution_root, outcome.execution_root);
        assert_eq!(attempt.trace_chunk_count, outcome.trace_chunk_count);
        assert_eq!(attempt.challenge, Hash64::default(), "the challenge is the nonce's, and the grind sets it");
    }
}
