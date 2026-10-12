//! **The output receipt: what a user keeps so that, later, nobody can change what they were told** (RFC-0001 + RFC-0003 delivery, task 4).
//!
//! A chat answer on a screen is not evidence of anything. The receipt is the small object the gateway hands back with a COMMITTED answer:
//! the claim id, the output root, the class, a digest of the decode rule, the executor bond, the tokenizer, a digest of the ids that make
//! the answer, a digest of the text that was shown, a digest of the canonical request and the chat template that produced the prompt — all
//! sealed under one `receipt_id` — and, once the rail has submitted it, the submission txid.
//!
//! # What "authenticated" means here, exactly
//!
//! The gateway holds no key (ADR-0079 Decision 4), so the receipt carries **no signature**. It is authenticated the only way a claim
//! can be: **by the chain**. [`verify_receipt`] re-derives every identity from the COMMITMENT (which anyone reads from the chain, from the
//! claim's transaction) and the output the user holds, and refuses on the first difference:
//!
//! 1. the receipt is internally sealed (`receipt_id` recomputes) — a field edited by hand is caught here;
//! 2. but a forger who re-seals is caught by the commitment: `claim_id`, `job_id`, `class_id`, `executor_bond`, `tokenizer_id`, the decode
//!    rule digest, `output_root` and `execution_root` must all equal the commitment's;
//! 3. the output the user holds hashes to the receipt's `output_token_ids_hash`, and — given the job context — recomputes to the
//!    commitment's `output_root` under the stated rule, so **changed output ids are refused even if the receipt was re-sealed**;
//! 4. the text the user was shown hashes to `shown_digest`, and (given a renderer) is a prefix of the rendering of the committed ids;
//! 5. [`check_against_chain`] compares the receipt with the node's claim row (`GetPalwFreePromptClaim`): a different output root, class or
//!    bond is refused; a row that matches is reported with its phase and the label `UNVERIFIED_REMOTE_STATE` — a node's word, not a proof.
//!
//! What a receipt does NOT establish: that the answer is *correct* (that is the Panel's replay and the court), that the claim will become
//! final (only the chain's `Final` says so — see `status`), or that the request the user typed produced this prompt (the receipt carries the
//! request digest and the template id so the gateway's operator can be asked, but no consensus rule checks the chat template).

use kaspa_consensus_core::palw_freeprompt_v3::{PalwFreePromptCommitmentV3, PalwFreePromptJobV3, fp_claim_id_v3, fp_job_id_v3};
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use kaspa_consensus_core::tx::TransactionOutpoint;
use kaspa_hashes::Hash64;

pub const SCHEMA: &str = "misaka.palw.output-receipt.v1";
pub const VERSION: u16 = 1;

const DOMAIN_ID: &[u8] = b"misaka-palw/gateway/output-receipt/v1";
const DOMAIN_OUTPUT_IDS: &[u8] = b"misaka-palw/gateway/receipt-output-ids/v1";
const DOMAIN_SHOWN: &[u8] = b"misaka-palw/gateway/receipt-shown/v1";
const DOMAIN_DECODE_RULE: &[u8] = b"misaka-palw/gateway/receipt-decode-rule/v1";

/// The sealed part of a receipt: facts fixed when the answer was committed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReceiptBodyV1 {
    pub network_domain: Hash64,
    pub claim_id: Hash64,
    pub job_id: Hash64,
    pub class_id: Hash64,
    pub executor_bond: TransactionOutpoint,
    pub tokenizer_id: Hash64,
    pub job_version: u16,
    /// [`decode_rule_digest`]: temperature, seed, ceiling and the whole decode config.
    pub decode_config_digest: Hash64,
    pub output_root: Hash64,
    pub execution_root: Hash64,
    /// The hash of the committed output token ids (what the answer IS).
    pub output_token_ids_hash: Hash64,
    pub output_tokens: u32,
    /// The hash of the text the user was shown (what the answer LOOKED LIKE).
    pub shown_digest: Hash64,
    /// The canonical digest of the request (`idempotency::request_digest`): which request this answers.
    pub request_digest: Hash64,
    /// The chat template that turned the request into the prompt. Not a claim field.
    pub template_id: String,
    pub job_context_hash: Hash64,
}

/// A receipt: the sealed body, its id, and the facts that arrive later.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReceiptV1 {
    pub body: ReceiptBodyV1,
    pub receipt_id: Hash64,
    /// The carrier transaction, once the rail has submitted it (`None` until then; outside the seal because it did not exist at sealing).
    pub submission_txid: Option<String>,
}

/// A preimage under a domain key: pieces appended, hashed with `kaspa_hashes::blake2b_512_keyed`.
struct Pre {
    domain: &'static [u8],
    buf: Vec<u8>,
}

impl Pre {
    fn new(domain: &'static [u8]) -> Self {
        Self { domain, buf: Vec::new() }
    }
    fn update(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }
    fn put(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
        self.buf.extend_from_slice(bytes);
    }
    fn finish(self) -> Hash64 {
        kaspa_hashes::blake2b_512_keyed(self.domain, &self.buf)
    }
}

/// The digest of the committed output ids.
pub fn output_ids_digest(ids: &[u32]) -> Hash64 {
    let mut state = Pre::new(DOMAIN_OUTPUT_IDS);
    state.update(&(ids.len() as u64).to_le_bytes());
    for id in ids {
        state.update(&id.to_le_bytes());
    }
    state.finish()
}

/// The digest of the text that was shown.
pub fn shown_digest(shown: &[u8]) -> Hash64 {
    let mut state = Pre::new(DOMAIN_SHOWN);
    state.put(shown);
    state.finish()
}

/// **The decode rule of a job, as one digest**: its version, sampler (temperature and seed R), decode ceiling and the whole
/// `DecodeConfigV4` (penalties, bias, stop sequences). Everything RFC-0001 §A froze about HOW the tokens were chosen, and nothing
/// about what they were.
pub fn decode_rule_digest(job: &PalwFreePromptJobV3) -> Hash64 {
    let mut state = Pre::new(DOMAIN_DECODE_RULE);
    state.update(&job.version.to_le_bytes());
    state.update(&job.temperature_q.to_le_bytes());
    state.update(&job.sampling_seed);
    state.update(&job.decode_token_limit.to_le_bytes());
    state.put(&borsh::to_vec(&job.decode).expect("a decode config serializes"));
    state.finish()
}

impl ReceiptBodyV1 {
    fn seal_id(&self) -> Hash64 {
        let mut state = Pre::new(DOMAIN_ID);
        state.update(&VERSION.to_le_bytes());
        for h in [self.network_domain, self.claim_id, self.job_id, self.class_id] {
            state.update(h.as_byte_slice());
        }
        state.update(self.executor_bond.transaction_id.as_bytes().as_slice());
        state.update(&self.executor_bond.index.to_le_bytes());
        state.update(self.tokenizer_id.as_byte_slice());
        state.update(&self.job_version.to_le_bytes());
        for h in [self.decode_config_digest, self.output_root, self.execution_root, self.output_token_ids_hash] {
            state.update(h.as_byte_slice());
        }
        state.update(&self.output_tokens.to_le_bytes());
        for h in [self.shown_digest, self.request_digest] {
            state.update(h.as_byte_slice());
        }
        state.put(self.template_id.as_bytes());
        state.update(self.job_context_hash.as_byte_slice());
        state.finish()
    }
}

impl ReceiptV1 {
    /// Seal a committed answer's receipt. `output_ids` and `shown` are the answer as delivered.
    pub fn seal(
        commitment: &PalwFreePromptCommitmentV3,
        output_ids: &[u32],
        shown: &[u8],
        request_digest: Hash64,
        template_id: &str,
        job_context_hash: Hash64,
    ) -> Self {
        let job = &commitment.job;
        let body = ReceiptBodyV1 {
            network_domain: job.network_domain,
            claim_id: fp_claim_id_v3(commitment),
            job_id: fp_job_id_v3(job),
            class_id: job.class_id,
            executor_bond: job.executor_bond,
            tokenizer_id: job.tokenizer_id,
            job_version: job.version,
            decode_config_digest: decode_rule_digest(job),
            output_root: commitment.output_root,
            execution_root: commitment.execution_root,
            output_token_ids_hash: output_ids_digest(output_ids),
            output_tokens: output_ids.len() as u32,
            shown_digest: shown_digest(shown),
            request_digest,
            template_id: template_id.to_string(),
            job_context_hash,
        };
        let receipt_id = body.seal_id();
        Self { body, receipt_id, submission_txid: None }
    }

    pub fn with_txid(mut self, txid: Option<String>) -> Self {
        self.submission_txid = txid;
        self
    }

    pub fn to_json(&self) -> serde_json::Value {
        let h = |x: &Hash64| faster_hex::hex_string(x.as_byte_slice());
        let b = &self.body;
        serde_json::json!({
            "schema": SCHEMA,
            "version": VERSION,
            "receipt_id": h(&self.receipt_id),
            "network_domain": h(&b.network_domain),
            "claim_id": h(&b.claim_id),
            "job_id": h(&b.job_id),
            "class_id": h(&b.class_id),
            "executor_bond": format!("{}:{}", b.executor_bond.transaction_id, b.executor_bond.index),
            "tokenizer_id": h(&b.tokenizer_id),
            "job_version": b.job_version,
            "decode_config_digest": h(&b.decode_config_digest),
            "output_root": h(&b.output_root),
            "execution_root": h(&b.execution_root),
            "output_token_ids_hash": h(&b.output_token_ids_hash),
            "output_tokens": b.output_tokens,
            "shown_digest": h(&b.shown_digest),
            "request_digest": h(&b.request_digest),
            "template_id": b.template_id,
            "job_context_hash": h(&b.job_context_hash),
            // Outside the seal: it did not exist when the receipt was sealed.
            "submission_txid": self.submission_txid,
            "authentication": "by the chain: verify against the claim's commitment (verify_receipt) and the node's claim row (check_against_chain); the gateway holds no key and signs nothing",
        })
    }

    pub fn from_json(v: &serde_json::Value) -> Result<Self, String> {
        if v.get("schema").and_then(serde_json::Value::as_str) != Some(SCHEMA) {
            return Err(format!("not a {SCHEMA}"));
        }
        let hash = |k: &str| -> Result<Hash64, String> {
            let s = v.get(k).and_then(serde_json::Value::as_str).ok_or_else(|| format!("receipt field `{k}` is missing"))?;
            let mut out = [0u8; 64];
            if s.len() != 128 || faster_hex::hex_decode(s.as_bytes(), &mut out).is_err() {
                return Err(format!("receipt field `{k}` is not 128 hex characters"));
            }
            Ok(Hash64::from_bytes(out))
        };
        let uint = |k: &str| v.get(k).and_then(serde_json::Value::as_u64).ok_or_else(|| format!("receipt field `{k}` is missing"));
        let bond = {
            let s = v.get("executor_bond").and_then(serde_json::Value::as_str).ok_or("receipt field `executor_bond` is missing")?;
            let (txid, index) = s.split_once(':').ok_or("receipt field `executor_bond` is not txid:index")?;
            let mut raw = [0u8; 64];
            if txid.len() != 128 || faster_hex::hex_decode(txid.as_bytes(), &mut raw).is_err() {
                return Err("receipt field `executor_bond` has a bad transaction id".into());
            }
            TransactionOutpoint::new(Hash64::from_bytes(raw), index.parse().map_err(|_| "receipt field `executor_bond` has a bad index")?)
        };
        let body = ReceiptBodyV1 {
            network_domain: hash("network_domain")?,
            claim_id: hash("claim_id")?,
            job_id: hash("job_id")?,
            class_id: hash("class_id")?,
            executor_bond: bond,
            tokenizer_id: hash("tokenizer_id")?,
            job_version: u16::try_from(uint("job_version")?).map_err(|_| "job_version out of range")?,
            decode_config_digest: hash("decode_config_digest")?,
            output_root: hash("output_root")?,
            execution_root: hash("execution_root")?,
            output_token_ids_hash: hash("output_token_ids_hash")?,
            output_tokens: u32::try_from(uint("output_tokens")?).map_err(|_| "output_tokens out of range")?,
            shown_digest: hash("shown_digest")?,
            request_digest: hash("request_digest")?,
            template_id: v.get("template_id").and_then(serde_json::Value::as_str).ok_or("receipt field `template_id` is missing")?.to_string(),
            job_context_hash: hash("job_context_hash")?,
        };
        Ok(Self {
            receipt_id: hash("receipt_id")?,
            submission_txid: v.get("submission_txid").and_then(serde_json::Value::as_str).map(str::to_string),
            body,
        })
    }
}

/// How the committed `output_root` is derived from the output ids — a property of the NETWORK (its attempt rule), named by the verifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputRule {
    /// `palw_attempt_output_root_v1(ctx, ids)`: testnet-12 (`CoreV1`), and the floor on every network.
    CoreV1,
}

impl OutputRule {
    fn root(self, ctx: &PalwJobContextV2, ids: &[u32]) -> Hash64 {
        match self {
            OutputRule::CoreV1 => kaspa_consensus_core::palw_attempt_rules_v1::palw_attempt_output_root_v1(ctx, ids),
        }
    }
}

/// What a verifier holds besides the receipt.
pub struct ReceiptWitness<'a> {
    /// The claim's commitment, as the chain holds it (from the carrier transaction's payload).
    pub commitment: &'a PalwFreePromptCommitmentV3,
    /// The output token ids the user holds.
    pub output_token_ids: &'a [u32],
    /// The text the user was shown, if the verifier has it.
    pub shown: Option<&'a [u8]>,
    /// The job's context (the response's `job_context`, borsh), to recompute `output_root` from the ids.
    pub job_context: Option<&'a PalwJobContextV2>,
    pub rule: OutputRule,
    /// The class tokenizer's rendering of ids, if the verifier has the tokenizer: the shown text must be a prefix of it.
    pub render: Option<&'a dyn Fn(&[u32]) -> Vec<u8>>,
}

/// Why a receipt is refused — every arm names the field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReceiptError {
    /// The receipt's own seal does not recompute: a field was edited.
    Unsealed,
    /// A field differs from the commitment's: the receipt is not for this claim.
    NotThisClaim(&'static str),
    /// The output the user holds is not the output the receipt names.
    OutputDiffers(&'static str),
    /// The shown text is not the text the receipt names (or not a prefix of the committed rendering).
    ShownDiffers(&'static str),
    /// The job context does not belong to this claim.
    ContextDiffers(&'static str),
}

impl std::fmt::Display for ReceiptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsealed => write!(f, "the receipt's seal does not recompute: a field was changed after it was sealed"),
            Self::NotThisClaim(field) => write!(f, "the receipt's {field} is not the commitment's: this receipt is not for this claim"),
            Self::OutputDiffers(why) => write!(f, "the output is not the one the receipt names: {why}"),
            Self::ShownDiffers(why) => write!(f, "the shown text is not the one the receipt names: {why}"),
            Self::ContextDiffers(why) => write!(f, "the job context does not belong to this claim: {why}"),
        }
    }
}

impl std::error::Error for ReceiptError {}

/// What a successful verification established (each flag is a check that RAN and passed; `false` means the witness did not offer it).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Verified {
    pub sealed: bool,
    pub claim_matches_commitment: bool,
    pub output_ids_match: bool,
    /// `output_root` recomputed from the ids and the job context equals the commitment's.
    pub output_root_recomputed: bool,
    pub shown_text_matches: bool,
    /// The shown text is a prefix of the rendering of the committed ids (needs a renderer).
    pub shown_is_a_rendering_of_the_ids: bool,
}

/// **Verify a receipt** against the claim's commitment and the output the user holds (see the module doc for the order and the meaning).
pub fn verify_receipt(receipt: &ReceiptV1, witness: &ReceiptWitness<'_>) -> Result<Verified, ReceiptError> {
    let mut verdict = Verified::default();
    // 1. The seal.
    if receipt.body.seal_id() != receipt.receipt_id {
        return Err(ReceiptError::Unsealed);
    }
    verdict.sealed = true;
    // 2. The commitment: every identity the receipt names, re-derived. A forger who re-sealed is stopped here.
    let c = witness.commitment;
    let b = &receipt.body;
    let checks: [(&'static str, bool); 11] = [
        ("claim_id", b.claim_id == fp_claim_id_v3(c)),
        ("job_id", b.job_id == fp_job_id_v3(&c.job)),
        ("network_domain", b.network_domain == c.job.network_domain),
        ("class_id", b.class_id == c.job.class_id),
        ("executor_bond", b.executor_bond == c.job.executor_bond),
        ("tokenizer_id", b.tokenizer_id == c.job.tokenizer_id),
        ("job_version", b.job_version == c.job.version),
        ("decode_config_digest", b.decode_config_digest == decode_rule_digest(&c.job)),
        ("output_root", b.output_root == c.output_root),
        ("execution_root", b.execution_root == c.execution_root),
        ("output_tokens", b.output_tokens == c.decode_tokens_executed),
    ];
    for (field, ok) in checks {
        if !ok {
            return Err(ReceiptError::NotThisClaim(field));
        }
    }
    verdict.claim_matches_commitment = true;
    // 3. The output.
    if witness.output_token_ids.len() != b.output_tokens as usize {
        return Err(ReceiptError::OutputDiffers("the number of ids"));
    }
    if output_ids_digest(witness.output_token_ids) != b.output_token_ids_hash {
        return Err(ReceiptError::OutputDiffers("the ids' digest"));
    }
    verdict.output_ids_match = true;
    if let Some(ctx) = witness.job_context {
        if ctx.context_hash() != b.job_context_hash {
            return Err(ReceiptError::ContextDiffers("its hash is not the receipt's"));
        }
        if ctx.job_id != b.job_id || ctx.prompt_token_ids_hash != c.job.prompt_token_ids_hash || ctx.tokenizer_id != c.job.tokenizer_id {
            return Err(ReceiptError::ContextDiffers("it names another job, prompt or tokenizer"));
        }
        if ctx.exact_decode_tokens != c.decode_tokens_executed {
            return Err(ReceiptError::ContextDiffers("its decode count is not the commitment's"));
        }
        if witness.rule.root(ctx, witness.output_token_ids) != c.output_root {
            return Err(ReceiptError::OutputDiffers("the ids do not recompute to the commitment's output_root"));
        }
        verdict.output_root_recomputed = true;
    }
    // 4. The text shown.
    if let Some(shown) = witness.shown {
        if shown_digest(shown) != b.shown_digest {
            return Err(ReceiptError::ShownDiffers("its digest"));
        }
        verdict.shown_text_matches = true;
        if let Some(render) = witness.render {
            let rendering = render(witness.output_token_ids);
            let rendering = String::from_utf8_lossy(&rendering).into_owned();
            if !rendering.starts_with(String::from_utf8_lossy(shown).as_ref()) {
                return Err(ReceiptError::ShownDiffers("it is not a rendering of the committed ids"));
            }
            verdict.shown_is_a_rendering_of_the_ids = true;
        }
    }
    Ok(verdict)
}

/// What the chain's claim row says of a receipt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChainStanding {
    /// The node holds no such claim (yet, or any more).
    NotOnChain,
    /// The node's row agrees with the receipt. `label` says how that was learned.
    Agrees { phase: String, label: &'static str },
}

/// **Check a receipt against a node's claim row** (`GetPalwFreePromptClaim`). A row that disagrees is refused by field; one that agrees is
/// `UNVERIFIED_REMOTE_STATE` — the node said so, and nothing here proves it (RFC-0009 §6).
pub fn check_against_chain(receipt: &ReceiptV1, row: &kaspa_rpc_core::GetPalwFreePromptClaimResponse) -> Result<ChainStanding, ReceiptError> {
    if !row.found {
        return Ok(ChainStanding::NotOnChain);
    }
    let hex = |h: &Hash64| faster_hex::hex_string(h.as_byte_slice());
    let b = &receipt.body;
    let bond = format!("{}:{}", b.executor_bond.transaction_id, b.executor_bond.index);
    let checks: [(&'static str, bool); 6] = [
        ("claim_id", row.claim_id == hex(&b.claim_id)),
        ("class_id", row.class_id == hex(&b.class_id)),
        ("output_root", row.output_root == hex(&b.output_root)),
        ("execution_root", row.execution_root == hex(&b.execution_root)),
        ("executor_bond", row.executor_bond == bond),
        ("is_free_prompt", row.is_free_prompt),
    ];
    for (field, ok) in checks {
        if !ok {
            return Err(ReceiptError::NotThisClaim(field));
        }
    }
    Ok(ChainStanding::Agrees { phase: row.phase.clone(), label: misaka_palw_remote::trust::UNVERIFIED_REMOTE_STATE })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::{FloorWorker, certified_facts, config, identity, offline_source, temp_dir};
    use crate::serving;
    use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;

    /// A real committed answer: the commitment from the outbox, the response body, and the job context from the worker's retention.
    struct Answer {
        commitment: PalwFreePromptCommitmentV3,
        ids: Vec<u32>,
        shown: Vec<u8>,
        context: PalwJobContextV2,
        body: serde_json::Value,
        dir: std::path::PathBuf,
    }

    fn answer() -> Answer {
        let dir = temp_dir("receipt");
        let w = FloorWorker::new(&dir.join("traces"), PalwPromptIdsFormV1::MerkleV1);
        let (cfg, id, facts) = (config(&dir), identity(&w.profile), certified_facts(false));
        let source = offline_source(&dir);
        let request = serde_json::json!({ "messages": [{ "role": "user", "content": "hi" }], "max_tokens": 4 });
        let body = crate::testkit::chat(&cfg, &id, &w, &facts, &source, &request, &serving::AlwaysPresent).expect("answers");
        assert_eq!(body["misaka"]["committed"], true);
        let stem = format!("fp-job-{}", &body["misaka"]["fp_job_id"].as_str().unwrap()[..16]);
        let commitment: PalwFreePromptCommitmentV3 = borsh::from_slice(&std::fs::read(dir.join(format!("{stem}.commitment-unsigned.borsh"))).unwrap()).unwrap();
        let ids: Vec<u32> = serde_json::from_value(body["misaka"]["output_token_ids"].clone()).unwrap();
        let shown = body["choices"][0]["message"]["content"].as_str().unwrap().as_bytes().to_vec();
        let ctx_hex = body["misaka"]["job_context"].as_str().expect("the response publishes the job context");
        let context: PalwJobContextV2 = borsh::from_slice(&from_hex(ctx_hex)).unwrap();
        Answer { commitment, ids, shown, context, body, dir }
    }

    fn from_hex(s: &str) -> Vec<u8> {
        let mut out = vec![0u8; s.len() / 2];
        faster_hex::hex_decode(s.as_bytes(), &mut out).expect("hex");
        out
    }

    fn witness<'a>(a: &'a Answer, shown: Option<&'a [u8]>) -> ReceiptWitness<'a> {
        ReceiptWitness { commitment: &a.commitment, output_token_ids: &a.ids, shown, job_context: Some(&a.context), rule: OutputRule::CoreV1, render: None }
    }

    fn receipt_of(a: &Answer) -> ReceiptV1 {
        ReceiptV1::from_json(&a.body["misaka"]["receipt"]).expect("the response carries a receipt")
    }

    #[test]
    fn a_committed_answer_carries_a_receipt_that_verifies_against_its_commitment_and_output() {
        let a = answer();
        let receipt = receipt_of(&a);
        // The fields the task names.
        let j = &a.body["misaka"]["receipt"];
        for k in ["claim_id", "output_root", "class_id", "decode_config_digest", "submission_txid", "receipt_id"] {
            assert!(j.get(k).is_some(), "the receipt names {k}");
        }
        assert_eq!(j["claim_id"], a.body["misaka"]["fp_claim_id"]);
        assert_eq!(j["output_root"], a.body["misaka"]["output_root"]);
        assert!(j["submission_txid"].is_null(), "not submitted yet: the txid arrives later, outside the seal");
        let verdict = verify_receipt(&receipt, &witness(&a, Some(&a.shown))).expect("the honest receipt verifies");
        assert_eq!(
            verdict,
            Verified {
                sealed: true,
                claim_matches_commitment: true,
                output_ids_match: true,
                output_root_recomputed: true,
                shown_text_matches: true,
                shown_is_a_rendering_of_the_ids: false
            },
            "every offered check ran; the rendering check needs a renderer"
        );
        // JSON round trip.
        assert_eq!(ReceiptV1::from_json(&receipt.to_json()).unwrap(), receipt);
        let with_tx = receipt.clone().with_txid(Some("ab".repeat(64)));
        assert_eq!(ReceiptV1::from_json(&with_tx.to_json()).unwrap().submission_txid.as_deref(), Some("ab".repeat(64).as_str()));
        verify_receipt(&with_tx, &witness(&a, None)).expect("the txid is outside the seal: adding it does not unseal");
        let _ = std::fs::remove_dir_all(&a.dir);
    }

    #[test]
    fn a_receipt_edited_by_hand_is_unsealed() {
        let a = answer();
        let receipt = receipt_of(&a);
        let h = |n: u64| Hash64::from_u64_word(n);
        let edits: Vec<(&str, Box<dyn Fn(&mut ReceiptV1)>)> = vec![
            ("network_domain", Box::new(|r| r.body.network_domain = h(1))),
            ("claim_id", Box::new(|r| r.body.claim_id = h(1))),
            ("job_id", Box::new(|r| r.body.job_id = h(1))),
            ("class_id", Box::new(|r| r.body.class_id = h(1))),
            ("executor_bond", Box::new(|r| r.body.executor_bond.index += 1)),
            ("tokenizer_id", Box::new(|r| r.body.tokenizer_id = h(1))),
            ("job_version", Box::new(|r| r.body.job_version += 1)),
            ("decode_config_digest", Box::new(|r| r.body.decode_config_digest = h(1))),
            ("output_root", Box::new(|r| r.body.output_root = h(1))),
            ("execution_root", Box::new(|r| r.body.execution_root = h(1))),
            ("output_token_ids_hash", Box::new(|r| r.body.output_token_ids_hash = h(1))),
            ("output_tokens", Box::new(|r| r.body.output_tokens += 1)),
            ("shown_digest", Box::new(|r| r.body.shown_digest = h(1))),
            ("request_digest", Box::new(|r| r.body.request_digest = h(1))),
            ("template_id", Box::new(|r| r.body.template_id.push('x'))),
            ("job_context_hash", Box::new(|r| r.body.job_context_hash = h(1))),
        ];
        for (what, edit) in edits {
            let mut forged = receipt.clone();
            edit(&mut forged);
            assert_eq!(verify_receipt(&forged, &witness(&a, None)).unwrap_err(), ReceiptError::Unsealed, "{what}");
        }
        let _ = std::fs::remove_dir_all(&a.dir);
    }

    /// The forger who re-seals is stopped by the commitment, the output and the shown text.
    #[test]
    fn a_resealed_forgery_is_refused_by_the_commitment_the_output_and_the_shown_text() {
        let a = answer();
        let receipt = receipt_of(&a);
        let reseal = |mut r: ReceiptV1, edit: &dyn Fn(&mut ReceiptBodyV1)| {
            edit(&mut r.body);
            r.receipt_id = r.body.seal_id();
            r
        };
        let h = Hash64::from_u64_word(0xBAD);
        // Fields the commitment fixes.
        for (field, edit) in [
            ("claim_id", Box::new(move |b: &mut ReceiptBodyV1| b.claim_id = h) as Box<dyn Fn(&mut ReceiptBodyV1)>),
            ("job_id", Box::new(move |b| b.job_id = h)),
            ("class_id", Box::new(move |b| b.class_id = h)),
            ("executor_bond", Box::new(|b| b.executor_bond.index += 1)),
            ("tokenizer_id", Box::new(move |b| b.tokenizer_id = h)),
            ("job_version", Box::new(|b| b.job_version += 1)),
            ("decode_config_digest", Box::new(move |b| b.decode_config_digest = h)),
            ("output_root", Box::new(move |b| b.output_root = h)),
            ("execution_root", Box::new(move |b| b.execution_root = h)),
            ("output_tokens", Box::new(|b| b.output_tokens += 1)),
            ("network_domain", Box::new(move |b| b.network_domain = h)),
        ] {
            let forged = reseal(receipt.clone(), &*edit);
            assert_eq!(verify_receipt(&forged, &witness(&a, None)).unwrap_err(), ReceiptError::NotThisClaim(field), "{field}");
        }
        // The output: other ids with the receipt's own hash are refused; ids AND a re-sealed hash are refused by the output root.
        let mut other_ids = a.ids.clone();
        other_ids[0] ^= 1;
        let mut w = witness(&a, None);
        w.output_token_ids = &other_ids;
        assert_eq!(verify_receipt(&receipt, &w).unwrap_err(), ReceiptError::OutputDiffers("the ids' digest"));
        let reforged = reseal(receipt.clone(), &|b| b.output_token_ids_hash = output_ids_digest(&other_ids));
        let err = verify_receipt(&reforged, &w).unwrap_err();
        assert_eq!(err, ReceiptError::OutputDiffers("the ids do not recompute to the commitment's output_root"), "a re-sealed receipt cannot launder other ids: {err}");
        // Fewer ids than the commitment executed.
        let mut short = witness(&a, None);
        short.output_token_ids = &a.ids[..a.ids.len() - 1];
        assert!(verify_receipt(&receipt, &short).is_err());
        // The shown text.
        let mut text = a.shown.clone();
        text.push(b'!');
        assert_eq!(verify_receipt(&receipt, &witness(&a, Some(&text))).unwrap_err(), ReceiptError::ShownDiffers("its digest"));
        let swapped = reseal(receipt.clone(), &|b| b.shown_digest = shown_digest(&text));
        // A re-sealed digest of different text passes the digest check — the renderer is what catches it.
        let render = |ids: &[u32]| -> Vec<u8> { ids.iter().filter(|i| **i < 256).map(|i| *i as u8).collect() };
        let mut with_render = witness(&a, Some(&text));
        with_render.render = Some(&render);
        assert_eq!(verify_receipt(&swapped, &with_render).unwrap_err(), ReceiptError::ShownDiffers("it is not a rendering of the committed ids"));
        let mut honest_render = witness(&a, Some(&a.shown));
        honest_render.render = Some(&render);
        assert!(verify_receipt(&receipt, &honest_render).unwrap().shown_is_a_rendering_of_the_ids);
        // The context.
        let mut other_ctx = a.context.clone();
        other_ctx.execution_seed[0] ^= 1;
        let mut wc = witness(&a, None);
        wc.job_context = Some(&other_ctx);
        assert_eq!(verify_receipt(&receipt, &wc).unwrap_err(), ReceiptError::ContextDiffers("its hash is not the receipt's"));
        // A different claim's commitment.
        let mut other_commitment = a.commitment.clone();
        other_commitment.job.job_nonce[0] ^= 1;
        let mut wo = witness(&a, None);
        wo.commitment = &other_commitment;
        assert_eq!(verify_receipt(&receipt, &wo).unwrap_err(), ReceiptError::NotThisClaim("claim_id"));
        let _ = std::fs::remove_dir_all(&a.dir);
    }

    #[test]
    fn the_nodes_claim_row_is_checked_against_the_receipt_and_an_agreeing_row_is_labelled_unverified() {
        let a = answer();
        let receipt = receipt_of(&a);
        let hex = |h: &Hash64| faster_hex::hex_string(h.as_byte_slice());
        let row = kaspa_rpc_core::GetPalwFreePromptClaimResponse {
            found: true,
            is_free_prompt: true,
            claim_id: hex(&receipt.body.claim_id),
            class_id: hex(&receipt.body.class_id),
            output_root: hex(&receipt.body.output_root),
            execution_root: hex(&receipt.body.execution_root),
            executor_bond: format!("{}:{}", receipt.body.executor_bond.transaction_id, receipt.body.executor_bond.index),
            phase: "receipt_licensed".into(),
            ..Default::default()
        };
        match check_against_chain(&receipt, &row).unwrap() {
            ChainStanding::Agrees { phase, label } => {
                assert_eq!((phase.as_str(), label), ("receipt_licensed", "UNVERIFIED_REMOTE_STATE"));
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(check_against_chain(&receipt, &kaspa_rpc_core::GetPalwFreePromptClaimResponse::default()).unwrap(), ChainStanding::NotOnChain);
        for (field, edit) in [
            ("output_root", Box::new(|r: &mut kaspa_rpc_core::GetPalwFreePromptClaimResponse| r.output_root = "00".repeat(64)) as Box<dyn Fn(&mut kaspa_rpc_core::GetPalwFreePromptClaimResponse)>),
            ("class_id", Box::new(|r| r.class_id = "00".repeat(64))),
            ("execution_root", Box::new(|r| r.execution_root = "00".repeat(64))),
            ("executor_bond", Box::new(|r| r.executor_bond = format!("{}:9", "00".repeat(64)))),
            ("claim_id", Box::new(|r| r.claim_id = "00".repeat(64))),
            ("is_free_prompt", Box::new(|r| r.is_free_prompt = false)),
        ] {
            let mut forged = row.clone();
            edit(&mut forged);
            assert_eq!(check_against_chain(&receipt, &forged).unwrap_err(), ReceiptError::NotThisClaim(field), "{field}");
        }
        let _ = std::fs::remove_dir_all(&a.dir);
    }
}
