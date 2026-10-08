//! Idempotent relay of bytes the miner already signed.
//!
//! The miner computes the identity of what it sends (tx id, block hash) **locally**, from the signed bytes. A node's reply is
//! only ever compared with that: a node that answers with another id, or that "accepts" something we cannot match, has not relayed
//! our bytes and is recorded as [`NodeOutcome::Tampered`]. Re-sending is safe because the id is a function of the signed fields —
//! "already known" is a success, not an error. An accept is **not** inclusion; that is [`crate::track`]'s business.

use kaspa_consensus_core::hashing::sighash::{Mldsa87SigHashReusedValuesUnsync, calc_mldsa87_signature_hash};
use kaspa_consensus_core::hashing::sighash_type::SIG_HASH_ALL;
use kaspa_consensus_core::tx::{MutableTransaction, Transaction, UtxoEntry};
use kaspa_hashes::Hash64;

/// What a node said about the submission.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reply {
    /// Accepted into the node's mempool / DAG; carries the id the NODE computed.
    Accepted(Hash64),
    /// The node already holds it (a previous send, or another relay); carries the id the node holds.
    AlreadyKnown(Hash64),
    /// A named refusal (mass, fee, orphan, double spend...). Not evidence of tampering.
    Refused(String),
    /// No answer (connection, timeout).
    Unreachable(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NodeOutcome {
    Accepted,
    AlreadyKnown,
    Refused(String),
    /// The node answered success for an id that is not ours: it relayed something else, or lied.
    Tampered { returned: Hash64 },
    Unreachable(String),
}

impl NodeOutcome {
    pub fn is_success(&self) -> bool {
        matches!(self, NodeOutcome::Accepted | NodeOutcome::AlreadyKnown)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelayReport {
    /// The identity we computed from our own bytes.
    pub id: Hash64,
    pub per_node: Vec<(String, NodeOutcome)>,
    pub successes: usize,
}

impl RelayReport {
    pub fn tampered(&self) -> Vec<&str> {
        self.per_node.iter().filter(|(_, o)| matches!(o, NodeOutcome::Tampered { .. })).map(|(n, _)| n.as_str()).collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RelayFailure {
    #[error("no node accepted: {0:?}")]
    NoneAccepted(RelayReport),
    #[error("only {got} of the required {need} node(s) accepted: {report:?}")]
    TooFew { got: usize, need: usize, report: RelayReport },
    #[error("the carrier failed the miner's own pre-flight: {0}")]
    Preflight(String),
}

/// Judge the replies against `expected`. `Ok` only when at least `min_accept` nodes returned our id; tampering is reported in the
/// report even on success (a miner should know a node lied to it).
pub fn fan_out_verdict(expected: Hash64, replies: Vec<(String, Reply)>, min_accept: usize) -> Result<RelayReport, RelayFailure> {
    let per_node: Vec<(String, NodeOutcome)> = replies
        .into_iter()
        .map(|(node, reply)| {
            let outcome = match reply {
                Reply::Accepted(id) if id == expected => NodeOutcome::Accepted,
                Reply::AlreadyKnown(id) if id == expected => NodeOutcome::AlreadyKnown,
                Reply::Accepted(id) | Reply::AlreadyKnown(id) => NodeOutcome::Tampered { returned: id },
                Reply::Refused(why) => NodeOutcome::Refused(why),
                Reply::Unreachable(why) => NodeOutcome::Unreachable(why),
            };
            (node, outcome)
        })
        .collect();
    let successes = per_node.iter().filter(|(_, o)| o.is_success()).count();
    let report = RelayReport { id: expected, per_node, successes };
    let need = min_accept.max(1);
    if successes == 0 {
        return Err(RelayFailure::NoneAccepted(report));
    }
    if successes < need {
        return Err(RelayFailure::TooFew { got: successes, need, report });
    }
    Ok(report)
}

/// The id of `tx` recomputed from its fields, ignoring the cached one (an in-process object edited without `finalize` carries a stale
/// id — the 2026-09-26 split — and a miner must hand a node the id of the bytes it is actually sending).
pub fn tx_id_of_bytes(tx: &Transaction) -> Hash64 {
    let mut fresh = tx.clone();
    fresh.finalize();
    fresh.id()
}

/// One node that takes a raw transaction.
pub trait RelayNode {
    fn node_id(&self) -> &str;
    fn submit_raw_tx(&self, tx: &Transaction) -> Reply;
}

/// The miner's own pre-flight of a carrier it is about to relay: the funding signature must verify over the transaction's own
/// sighash against the funding entry the miner believes it spends. Catches a bad signer output (or a sidecar that signed other
/// bytes) before a fee is risked, and is the same check the chain's script engine will make.
pub fn carrier_funding_signature_valid(tx: &Transaction, funding: &UtxoEntry) -> Result<(), String> {
    if tx.inputs.len() != 1 {
        return Err(format!("a carrier built by the rail has one funding input, this has {}", tx.inputs.len()));
    }
    let script = &tx.inputs[0].signature_script;
    let (sig_ht, pubkey) = parse_two_pushes(script).ok_or("the signature script is not <signature‖hashtype> <pubkey>")?;
    let (sig, hash_type) = sig_ht.split_at(sig_ht.len().saturating_sub(1));
    if hash_type != [SIG_HASH_ALL.to_u8()] {
        return Err("the funding signature is not SIG_HASH_ALL: a relay could alter what it does not cover".into());
    }
    let mtx = MutableTransaction::with_entries(tx.clone(), vec![funding.clone()]);
    let digest = calc_mldsa87_signature_hash(&mtx.as_verifiable(), 0, SIG_HASH_ALL, &Mldsa87SigHashReusedValuesUnsync::new());
    match kaspa_txscript::verify_mldsa87_with_context(pubkey, digest.as_bytes().as_slice(), sig, kaspa_txscript::MLDSA87_TX_CONTEXT) {
        Ok(true) => Ok(()),
        Ok(false) => Err("the funding signature does not verify over the transaction".into()),
        Err(e) => Err(format!("the funding signature is malformed: {e:?}")),
    }
}

/// The public key a carrier's single funding input is signed with, read from its signature script (`<sig‖hashtype> <pubkey>`). A relay that holds
/// a completed carrier needs it to rebuild the funding entry (the script the sighash commits to) from the amount alone.
pub fn funding_pubkey_of_carrier_v1(tx: &Transaction) -> Option<Vec<u8>> {
    let [input] = tx.inputs.as_slice() else { return None };
    parse_two_pushes(&input.signature_script).map(|(_, pk)| pk.to_vec())
}

/// What a relay learns from a completed free-prompt carrier it did not sign.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckedFpCarrierV1 {
    pub claim_id: Hash64,
    pub executor_pubkey: Vec<u8>,
    pub funding_pubkey: Vec<u8>,
}

/// **Is this a free-prompt carrier worth relaying?** A relay holds no key and is no authority, but it should not be a conduit for bytes that cannot
/// stand: the transaction rides the free-prompt subnetwork, its payload decodes, and the claim signature verifies under the executor key the
/// commitment itself names. The funding signature is checked by [`broadcast_signed_tx`] once the funding amount is known (the sighash commits to it).
pub fn check_fp_carrier_v1(tx: &Transaction) -> Result<CheckedFpCarrierV1, String> {
    use kaspa_consensus_core::palw_freeprompt_v3::{PALW_FP_V3_MLDSA87_COMMITMENT_CONTEXT, PalwFpCommitmentTxPayloadV3};
    if tx.subnetwork_id != kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_FP_COMMITMENT {
        return Err(format!("the transaction rides subnetwork {}, not the free-prompt commitment's", tx.subnetwork_id));
    }
    let payload: PalwFpCommitmentTxPayloadV3 = borsh::from_slice(&tx.payload).map_err(|e| format!("the payload does not decode: {e}"))?;
    let claim_id = payload.claim_id();
    let executor_pubkey = payload.commitment.job.executor_pubkey.clone();
    if !matches!(
        kaspa_txscript::verify_mldsa87_with_context(
            &executor_pubkey,
            claim_id.as_byte_slice(),
            &payload.signature,
            PALW_FP_V3_MLDSA87_COMMITMENT_CONTEXT
        ),
        Ok(true)
    ) {
        return Err("the claim signature does not verify under the executor key the commitment names".into());
    }
    let funding_pubkey = funding_pubkey_of_carrier_v1(tx).ok_or("the carrier does not have one funding input signed with <signature> <pubkey>")?;
    Ok(CheckedFpCarrierV1 { claim_id, executor_pubkey, funding_pubkey })
}

fn read_push(script: &[u8]) -> Option<(&[u8], &[u8])> {
    let (&op, rest) = script.split_first()?;
    let (len, rest) = match op {
        1..=75 => (op as usize, rest),
        0x4c => (*rest.first()? as usize, rest.get(1..)?),
        0x4d => (u16::from_le_bytes([*rest.first()?, *rest.get(1)?]) as usize, rest.get(2..)?),
        _ => return None,
    };
    (rest.len() >= len).then(|| rest.split_at(len))
}

pub(crate) fn parse_two_pushes(script: &[u8]) -> Option<(&[u8], &[u8])> {
    let (first, rest) = read_push(script)?;
    let (second, tail) = read_push(rest)?;
    tail.is_empty().then_some((first, second))
}

/// Send one signed carrier to every node and judge the replies. Pre-flights the signature when the funding entry is known.
pub fn broadcast_signed_tx(
    tx: &Transaction,
    funding: Option<&UtxoEntry>,
    nodes: &[&dyn RelayNode],
    min_accept: usize,
) -> Result<RelayReport, RelayFailure> {
    if let Some(entry) = funding {
        carrier_funding_signature_valid(tx, entry).map_err(RelayFailure::Preflight)?;
    }
    // Recomputed from the bytes, never the cached `tx.id()`: an in-process object edited without `finalize` carries a stale id.
    let expected = tx_id_of_bytes(tx);
    let mut seen = std::collections::BTreeSet::new();
    let replies = nodes
        .iter()
        .filter(|n| seen.insert(n.node_id().to_string()))
        .map(|n| (n.node_id().to_string(), n.submit_raw_tx(tx)))
        .collect();
    fan_out_verdict(expected, replies, min_accept)
}

#[cfg(test)]
pub(crate) mod fake {
    use super::*;
    use std::cell::RefCell;
    pub enum Mode {
        Honest,
        Refuse(&'static str),
        Down,
        ReturnsOtherId,
    }
    pub struct FakeRelay {
        pub id: String,
        pub mode: Mode,
        pub known: RefCell<bool>,
        pub sent: RefCell<Vec<Transaction>>,
    }
    impl FakeRelay {
        pub fn new(id: &str, mode: Mode) -> Self {
            Self { id: id.into(), mode, known: RefCell::new(false), sent: RefCell::new(vec![]) }
        }
    }
    impl RelayNode for FakeRelay {
        fn node_id(&self) -> &str {
            &self.id
        }
        fn submit_raw_tx(&self, tx: &Transaction) -> Reply {
            self.sent.borrow_mut().push(tx.clone());
            match self.mode {
                Mode::Down => Reply::Unreachable("timeout".into()),
                Mode::Refuse(why) => Reply::Refused(why.into()),
                Mode::ReturnsOtherId => Reply::Accepted(Hash64::from_bytes([0x66; 64])),
                Mode::Honest => {
                    // A real node recomputes the id from the bytes it received.
                    let id = tx.id();
                    if self.known.replace(true) { Reply::AlreadyKnown(id) } else { Reply::Accepted(id) }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::*;
    use super::*;
    use kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE;

    fn tx() -> Transaction {
        Transaction::new(0, vec![], vec![], 0, SUBNETWORK_ID_NATIVE, 0, vec![1, 2, 3])
    }
    fn refs(v: &[FakeRelay]) -> Vec<&dyn RelayNode> {
        v.iter().map(|n| n as &dyn RelayNode).collect()
    }

    #[test]
    fn honest_nodes_accept_and_a_second_send_is_idempotent() {
        let nodes = [FakeRelay::new("a", Mode::Honest), FakeRelay::new("b", Mode::Honest)];
        let r1 = broadcast_signed_tx(&tx(), None, &refs(&nodes), 2).unwrap();
        assert_eq!(r1.successes, 2);
        assert_eq!(r1.id, tx().id());
        let r2 = broadcast_signed_tx(&tx(), None, &refs(&nodes), 2).unwrap();
        assert!(r2.per_node.iter().all(|(_, o)| *o == NodeOutcome::AlreadyKnown), "double submission is a no-op success");
    }

    #[test]
    fn a_node_that_returns_another_id_is_tampering_and_never_counts() {
        let nodes = [FakeRelay::new("a", Mode::Honest), FakeRelay::new("liar", Mode::ReturnsOtherId)];
        let r = broadcast_signed_tx(&tx(), None, &refs(&nodes), 1).unwrap();
        assert_eq!(r.successes, 1);
        assert_eq!(r.tampered(), vec!["liar"]);
        // …and when only the liar answers, there is no success at all.
        let only = [FakeRelay::new("liar", Mode::ReturnsOtherId)];
        assert!(matches!(broadcast_signed_tx(&tx(), None, &refs(&only), 1), Err(RelayFailure::NoneAccepted(_))));
    }

    #[test]
    fn refusals_and_silence_are_reported_not_hidden_and_min_accept_is_enforced() {
        let nodes = [FakeRelay::new("a", Mode::Honest), FakeRelay::new("b", Mode::Refuse("orphan")), FakeRelay::new("c", Mode::Down)];
        assert!(matches!(broadcast_signed_tx(&tx(), None, &refs(&nodes), 2), Err(RelayFailure::TooFew { got: 1, need: 2, .. })));
        let ok = broadcast_signed_tx(&tx(), None, &refs(&nodes), 1).unwrap();
        assert!(ok.per_node.iter().any(|(n, o)| n == "b" && *o == NodeOutcome::Refused("orphan".into())));
    }

    #[test]
    fn a_relay_that_mutates_the_bytes_changes_the_id_so_the_miner_sees_a_different_tx() {
        // The miner's expected id is computed from ITS bytes. A node that forwards altered bytes answers with the altered id.
        let mut altered = tx();
        altered.payload[0] ^= 1;
        altered.finalize();
        assert_ne!(altered.id(), tx().id());
        let r = fan_out_verdict(tx().id(), vec![("n".into(), Reply::Accepted(altered.id()))], 1);
        assert!(matches!(r, Err(RelayFailure::NoneAccepted(_))));
    }

    // ---- a completed free-prompt carrier somebody else signed, relayed by anyone ----

    fn fp_carrier(amount: u64) -> (Transaction, UtxoEntry) {
        use kaspa_consensus_core::palw_freeprompt_v3::{
            PALW_FP_V3_VERSION, PalwFpStopReasonV3, PalwFreePromptCommitmentV3, PalwFreePromptJobV3, fp_trace_manifest_v3,
        };
        use kaspa_pq_validator_core::{FpCommitmentPriceV1, ValidatorKey, build_fp_commitment_tx_with};
        let key = ValidatorKey::from_seed([0x31; 32]);
        let network_domain = Hash64::from_bytes([0x4E; 64]);
        let ids: Vec<u32> = (0..96).collect();
        let job = PalwFreePromptJobV3 {
            version: PALW_FP_V3_VERSION,
            network_domain,
            class_id: Hash64::from_bytes([0xBA; 64]),
            executor_bond: kaspa_consensus_core::tx::TransactionOutpoint::new(Hash64::from_bytes([7; 64]), 0),
            executor_pubkey: key.public_key().to_vec(),
            operator_id: Hash64::from_bytes([0xE0; 64]),
            anchor_block: Hash64::from_bytes([0xA0; 64]),
            anchor_daa: 5_000,
            job_nonce: [0x11; 32],
            tokenizer_id: Hash64::from_bytes([0x70; 64]),
            prompt_token_ids_hash: kaspa_consensus_core::palw_v2::prompt_token_ids_hash_v2(&ids),
            prompt_tokens: ids.len() as u32,
            decode_token_limit: 512,
            max_context_tokens: 4_096,
            privacy_mode: kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_PRIVACY_PUBLIC_DA,
            prompt_mode: kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_PROMPT_MODE_USER,
            sampling_seed: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_SEED_GREEDY,
            temperature_q: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_TEMPERATURE_GREEDY,
            decode: None,
            tail: None,
        };
        let events: Vec<Hash64> = (0..256u64).map(|i| Hash64::from_u64_word(i + 1)).collect();
        let (manifest_root, chunk_count, _) = fp_trace_manifest_v3(Hash64::from_bytes([0xB1; 64]), &events);
        let commitment = PalwFreePromptCommitmentV3 {
            trace_root: Hash64::from_bytes([0x7A; 64]),
            output_root: Hash64::from_bytes([0x0B; 64]),
            schedule_root: Hash64::from_bytes([0x5C; 64]),
            execution_root: Hash64::from_bytes([0x4E; 64]),
            decode_tokens_executed: 256,
            stop_reason: PalwFpStopReasonV3::EndOfGeneration,
            work_leaves: 1_600,
            trace_manifest_root: manifest_root,
            trace_chunk_count: chunk_count,
            trace_retention_daa: 505_000,
            job,
        };
        let spk = crate::bundle::funding_spk_of_pubkey_v1(key.public_key());
        let entry = UtxoEntry::new(amount, spk, 0, false);
        let tx = build_fp_commitment_tx_with(
            &key,
            network_domain,
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            commitment,
            ids,
            FpCommitmentPriceV1::Chain { quanta: 16, pwu: 1_600 },
            kaspa_consensus_core::tx::TransactionOutpoint::new(Hash64::from_bytes([9; 64]), 0),
            &entry,
            300_000,
        )
        .expect("an admissible carrier builds");
        (tx, entry)
    }

    #[test]
    fn a_relay_forwards_a_carrier_it_did_not_sign_and_refuses_one_that_cannot_stand() {
        let (tx, entry) = fp_carrier(50_000_000);
        let checked = check_fp_carrier_v1(&tx).expect("a well-formed carrier");
        assert_eq!(checked.executor_pubkey, checked.funding_pubkey, "this carrier is funded by the executor's own key");
        // The funding entry is rebuilt from the public amount and the signing key alone — no key on the relay.
        let rebuilt = UtxoEntry::new(50_000_000, crate::bundle::funding_spk_of_pubkey_v1(&checked.funding_pubkey), 0, false);
        assert_eq!(rebuilt.script_public_key, entry.script_public_key);
        let nodes = [fake::FakeRelay::new("a", fake::Mode::Honest), fake::FakeRelay::new("b", fake::Mode::Honest)];
        let refs: Vec<&dyn RelayNode> = nodes.iter().map(|n| n as &dyn RelayNode).collect();
        let report = broadcast_signed_tx(&tx, Some(&rebuilt), &refs, 2).expect("relayed");
        assert_eq!(report.id, tx_id_of_bytes(&tx));
        // A WRONG funding amount: the funding signature (which commits to it) does not verify, so nothing is sent.
        let wrong = UtxoEntry::new(50_000_001, rebuilt.script_public_key.clone(), 0, false);
        assert!(matches!(broadcast_signed_tx(&tx, Some(&wrong), &refs, 2), Err(RelayFailure::Preflight(_))));
        // A flipped claim-signature bit: refused before any node is asked.
        let mut bad = tx.clone();
        let mut payload: kaspa_consensus_core::palw_freeprompt_v3::PalwFpCommitmentTxPayloadV3 = borsh::from_slice(&bad.payload).unwrap();
        payload.signature[10] ^= 1;
        bad.payload = borsh::to_vec(&payload).unwrap();
        bad.finalize();
        assert!(check_fp_carrier_v1(&bad).unwrap_err().contains("claim signature"));
        // Not a free-prompt carrier at all.
        let plain = Transaction::new(0, vec![], vec![], 0, kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE, 0, vec![]);
        assert!(check_fp_carrier_v1(&plain).is_err());
    }
}
