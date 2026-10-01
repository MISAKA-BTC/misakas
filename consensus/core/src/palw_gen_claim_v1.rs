//! **RFC-0003 §I.4: the tensor claim on the free-prompt lane — FP job version 10.** Dormant behind
//! `Params::palw_fp_job_v5` over `palw_gen_v1`: no preset carries the fence, so every door's first line
//! returns and FP Jobs V3 and V4 are exactly what they were.
//!
//! A tensor job ([`PalwGenJobV1`]: an image, an embedding) reaches the chain as a free-prompt commitment
//! whose job is the lane's one job type ([`PalwFreePromptJobV3`]) at [`PALW_FP_GEN_VERSION`]:
//!
//! ```text
//! the V3 fields (the envelope's, canonical zeros for the text fields it has not) ‖ PalwGenJobTailV1 { seed, body }
//! ```
//!
//! and nothing else — no decode config, no V5 tail. The commitment, the payload, the claim id and the
//! signed message are the lane's, with no layout change; **the job's id is [`palw_gen_job_id_v1`] of the
//! reconstructed job** ([`PalwGenJobV1::from_carried`]), so no field of the envelope, the seed or the body
//! can change after the fact. One encoding per behaviour: a non-zero text field in the shell is refused by
//! name ([`PalwGenClaimErrorV1::ShellNotCanonical`]), never read.
//!
//! What a commitment carries (§I.4.2): `trace_root` the step root, `output_root` the canonical output
//! root, `execution_root` the **tensor execution root of its own fields** (so acceptance recomputes it),
//! no executed decode tokens (`ExactBudgetReached`), no schedule root, one trace chunk and no manifest, no
//! retention (the chain derives it). Under `PublicDa` the payload's ids are the job's prompt ids followed
//! by its negative ids; under `PanelDa` none. Images never ride the chain.
//!
//! The three doors, all dormant: the isolation door ([`validate_palw_fp_gen_commitment_tx_v1`], height-free,
//! only where the ruleset carries the fence — everywhere else the lane's own door refuses version 10 by
//! name), the header-context door ([`palw_fp_gen_refusal_at_v1`], the containing block's height) and the
//! acceptance walk ([`palw_fp_gen_objects_from_accepted_txs_v1`], total over whatever was accepted). The
//! fold's tensor branch (`palw_gen_claim_fold_v1`) derives the class, the work and the capacity, and
//! writes the weightless claim.

use borsh::{BorshDeserialize, BorshSerialize};

use crate::Hash64;
use crate::palw_freeprompt_v3::{
    PALW_FP_PRIVACY_PANEL_DA, PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_PROMPT_MODE_USER, PalwFpCommitmentTxPayloadV3, PalwFpJobTailV1,
    PalwFpStopReasonV3, PalwFpV3Error, PalwFreePromptCommitmentV3, PalwFreePromptJobV3,
};
use crate::palw_fp_objects_v3::{PalwConsensusObjectV3Carrier, PalwFpClassCapsV1, PalwFpExtractionV3};
use crate::palw_gen_close_v1::palw_gen_tensor_execution_root_v1;
use crate::palw_gen_job_v1::{PALW_GEN_JOB_VERSION_V1, PalwGenBodyV1, PalwGenJobV1, PalwJobEnvelopeV1, palw_gen_job_id_v1};
use crate::palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_match_v1};
use crate::palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2};
use crate::subnets::SUBNETWORK_ID_PALW_FP_COMMITMENT;
use crate::tx::{Transaction, TransactionOutpoint};

/// **FP job version 10** (RFC-0003 §I.4): a tensor job. 5 is V3, 6 ADR-0096 D8's constraint job (named,
/// unbuilt), 7 is V4, 8 is V5, 9 is RFC-0004's evaluation job. Provisional as 8 is: RFC-0001 may renumber
/// at freeze, and it binds nothing a network runs until the fence arms.
pub const PALW_FP_GEN_VERSION: u16 = 10;
/// Key of [`palw_gen_work_id_v1`].
pub const PALW_GEN_WORK_ID_DOMAIN_V1: &[u8] = b"misaka-palw/gen/work-id/v1";
/// Key of the id a version-10 job gets when it is not a well-formed tensor job (so an id is total over
/// whatever a struct holds, and never a panic in a hash). No honest job has one.
pub const PALW_GEN_MALFORMED_JOB_ID_DOMAIN_V1: &[u8] = b"misaka-palw/gen/fp-job-id/malformed/v1";

fn keyed64(key: &[u8], parts: &[&[u8]]) -> Hash64 {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(key).to_state();
    for part in parts {
        state.update(part);
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// **A tensor job's tail** — what version 10 adds after the V3 fields: the job's seed (R's key, §I.1) and
/// its body.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwGenJobTailV1 {
    pub seed: [u8; 32],
    pub body: PalwGenBodyV1,
}

/// **Why a tensor claim is refused** — every refusal by name, none clamps or corrects.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwGenClaimErrorV1 {
    #[error("the job is version {version}, not a tensor job (version 10) with its tail and no decode rules")]
    NotATensorJob { version: u16 },
    #[error("a tensor job's lane field is not canonical: {0} (its text fields are zero: the class binds the tokenizer, the body carries the ids' commitments, the seed is the tail's)")]
    ShellNotCanonical(&'static str),
    #[error("unsupported payload version {got} (expected {expected})")]
    PayloadVersion { got: u16, expected: u16 },
    #[error("the job's network domain is not this network's")]
    NetworkDomainMismatch,
    #[error("privacy mode {0} is not PublicDa (1) or PanelDa (2)")]
    PrivacyModeNotOffered(u8),
    #[error("privacy mode 2 (PanelDa) is not armed on this network")]
    PanelDaNotArmed,
    #[error("prompt mode {0} is not the user's (0): a generative class has no canonical prompt in v1")]
    PromptModeNotOffered(u8),
    #[error("the executor's public key is missing")]
    MissingPublicKey,
    #[error("the signature is {got} bytes; ML-DSA-87's is {expected}")]
    SignatureLength { got: usize, expected: usize },
    #[error("{what}: {tokens} ids is not 0 exactly when the hash is zero")]
    EmptyIdsEncoding { what: &'static str, tokens: u32 },
    #[error("a tensor commitment's {0} is not canonical (§I.4.2)")]
    CommitmentNotCanonical(&'static str),
    #[error("a tensor commitment's {0} is zero")]
    ZeroRoot(&'static str),
    #[error("a tensor claim of no leaves")]
    ZeroWorkLeaves,
    #[error("{got} work leaves is above the cap {max}")]
    WorkLeavesAboveCap { got: u64, max: u64 },
    #[error("the commitment's execution_root is not the tensor execution root of its own parts")]
    ExecutionRootNotItsParts,
    #[error("PanelDa carries no ids: the payload carries {0}")]
    PanelDaPayloadCarriesIds(usize),
    #[error("the payload carries {got} ids for a job that says {declared} (its prompt's and its negative prompt's)")]
    IdsCount { got: usize, declared: u64 },
    #[error("the carried {what} ids are not the ones the job's commitment binds")]
    IdsNotTheJobs { what: &'static str },
    #[error("the payload does not decode as a free-prompt commitment")]
    PayloadUndecodable,
}

impl From<PalwGenClaimErrorV1> for PalwFpV3Error {
    fn from(e: PalwGenClaimErrorV1) -> Self {
        PalwFpV3Error::TensorClaim(e.to_string())
    }
}

// ---------------------------------------------------------------------------------------------
// The job as the lane carries it
// ---------------------------------------------------------------------------------------------

impl PalwGenJobV1 {
    /// This job's tail: what version 10 carries after the V3 fields.
    pub fn tail(&self) -> PalwGenJobTailV1 {
        PalwGenJobTailV1 { seed: self.seed, body: self.body.clone() }
    }

    /// **A tensor job as the lane carries it** (§I.4.1): the lane's job type at version 10 — the
    /// envelope's fields, canonical zeros for the text fields the envelope has not, no decode config, the
    /// tail.
    pub fn into_carried(&self) -> PalwFreePromptJobV3 {
        let e = &self.envelope;
        PalwFreePromptJobV3 {
            version: PALW_FP_GEN_VERSION,
            network_domain: e.network_domain,
            class_id: e.class_id,
            executor_bond: e.executor_bond,
            executor_pubkey: e.executor_pubkey.clone(),
            operator_id: e.operator_id,
            anchor_block: e.anchor_block,
            anchor_daa: e.anchor_daa,
            job_nonce: e.job_nonce,
            tokenizer_id: Hash64::default(),
            prompt_token_ids_hash: Hash64::default(),
            prompt_tokens: 0,
            decode_token_limit: 0,
            max_context_tokens: 0,
            privacy_mode: e.privacy_mode,
            prompt_mode: e.prompt_mode,
            sampling_seed: [0u8; 32],
            temperature_q: 0,
            decode: None,
            tail: Some(PalwFpJobTailV1::Gen(Box::new(self.tail()))),
        }
    }

    /// **The tensor job a carried job is**, if it is one: version 10, no decode rules, the tail present,
    /// and every text field of the shell the canonical zero. Total: a job that is not one is a named
    /// refusal.
    pub fn from_carried(job: &PalwFreePromptJobV3) -> Result<Self, PalwGenClaimErrorV1> {
        let Some(tail) = Self::tail_of_carried(job) else {
            return Err(PalwGenClaimErrorV1::NotATensorJob { version: job.version });
        };
        use PalwGenClaimErrorV1::ShellNotCanonical as Shell;
        if job.tokenizer_id != Hash64::default() {
            return Err(Shell("tokenizer_id"));
        }
        if job.prompt_token_ids_hash != Hash64::default() || job.prompt_tokens != 0 {
            return Err(Shell("the prompt commitment (the body carries it)"));
        }
        if job.decode_token_limit != 0 || job.max_context_tokens != 0 {
            return Err(Shell("decode_token_limit / max_context_tokens"));
        }
        if job.sampling_seed != [0u8; 32] || job.temperature_q != 0 {
            return Err(Shell("sampling_seed / temperature_q (the seed is the tail's)"));
        }
        Ok(Self::reconstruct(job, tail))
    }

    /// The tail of a carried tensor job (version 10, no decode rules, a tensor tail), without any
    /// canonical-form check.
    fn tail_of_carried(job: &PalwFreePromptJobV3) -> Option<&PalwGenJobTailV1> {
        match (&job.tail, job.version == PALW_FP_GEN_VERSION && job.decode.is_none()) {
            (Some(PalwFpJobTailV1::Gen(tail)), true) => Some(tail.as_ref()),
            _ => None,
        }
    }

    fn reconstruct(job: &PalwFreePromptJobV3, tail: &PalwGenJobTailV1) -> Self {
        Self {
            version: PALW_GEN_JOB_VERSION_V1,
            envelope: PalwJobEnvelopeV1 {
                network_domain: job.network_domain,
                class_id: job.class_id,
                executor_bond: job.executor_bond,
                executor_pubkey: job.executor_pubkey.clone(),
                operator_id: job.operator_id,
                anchor_block: job.anchor_block,
                anchor_daa: job.anchor_daa,
                job_nonce: job.job_nonce,
                privacy_mode: job.privacy_mode,
                prompt_mode: job.prompt_mode,
            },
            seed: tail.seed,
            body: tail.body.clone(),
        }
    }
}

/// **`fp_job_id_v3` of a version-10 job** (the lane's one job-id function dispatches here): the
/// generative job's own id, [`palw_gen_job_id_v1`], of the reconstruction. A job that is not a
/// well-formed tensor job (no tail, a decode config) has an id too — under its own key over the carried
/// bytes — so an id is total over whatever a struct holds, and a malformed job is refused by name by
/// validation, never by a panic in a hash.
pub fn fp_job_id_gen_carried_v1(job: &PalwFreePromptJobV3) -> Hash64 {
    match PalwGenJobV1::tail_of_carried(job) {
        Some(tail) => palw_gen_job_id_v1(&PalwGenJobV1::reconstruct(job, tail)),
        None => {
            let bytes = borsh::to_vec(job).expect("a free-prompt job is borsh-serializable");
            keyed64(PALW_GEN_MALFORMED_JOB_ID_DOMAIN_V1, &[&bytes])
        }
    }
}

/// **A tensor claim's work identity** (§I.4.5, step 6): `H64(key "misaka-palw/gen/work-id/v1", class_id ‖
/// borsh(tail) ‖ executor bond)`. One inference is one claim per bond: the same job under another nonce
/// or anchor is the same work, and another bond's run of the job is another.
pub fn palw_gen_work_id_v1(class_id: &Hash64, tail: &PalwGenJobTailV1, bond: &TransactionOutpoint) -> Hash64 {
    let tail = borsh::to_vec(tail).expect("a tensor job's tail is borsh-serializable");
    keyed64(
        PALW_GEN_WORK_ID_DOMAIN_V1,
        &[class_id.as_byte_slice(), &tail, bond.transaction_id.as_bytes().as_slice(), &bond.index.to_le_bytes()],
    )
}

// ---------------------------------------------------------------------------------------------
// The commitment's own rules
// ---------------------------------------------------------------------------------------------

/// **The ids a payload carries, split** at the job's prompt count: `(prompt ids, negative ids)`.
pub fn palw_gen_split_ids_v1(ids: &[u32], prompt_tokens: u32) -> (&[u32], &[u32]) {
    ids.split_at((prompt_tokens as usize).min(ids.len()))
}

/// **Is this FP payload a tensor claim's?** Its job's version word — the payload's bytes 2..4, after the
/// payload's own version — is [`PALW_FP_GEN_VERSION`]. Nothing else is read.
pub fn palw_fp_payload_is_gen_v1(payload: &[u8]) -> bool {
    payload.get(2..4) == Some(&PALW_FP_GEN_VERSION.to_le_bytes()[..])
}

/// **The tensor commitment's tensor execution root** — a function of the commitment's own fields
/// (§I.4.2), which acceptance recomputes.
pub fn palw_gen_commitment_execution_root_v1(commitment: &PalwFreePromptCommitmentV3, job: &PalwGenJobV1) -> Hash64 {
    palw_gen_tensor_execution_root_v1(
        &palw_gen_job_id_v1(job),
        &commitment.job.class_id,
        commitment.work_leaves,
        &commitment.trace_root,
        &commitment.output_root,
    )
}

/// **A tensor claim's stateless rules** (§I.4.4, the isolation door's and the walk's): the payload and
/// its job are the version-10 form, the network's (where known), canonical in themselves; the commitment's
/// fields are §I.4.2's; its execution root is its parts'; its ids (where carried) are the body's. Returns
/// the reconstructed job. The signature is verified by the caller (this crate holds no ML-DSA
/// implementation).
///
/// `network_domain` is `None` for the height-free door, which holds none; `max_step_leaf_count` is the
/// class's ladder at the walk and the structural cap at the door (a caller with no ruleset of its own
/// must not be stricter than the walk).
pub fn palw_fp_gen_claim_check_v1(
    payload: &PalwFpCommitmentTxPayloadV3,
    network_domain: Option<Hash64>,
    panel_da_armed: bool,
    max_step_leaf_count: u64,
    prompt_ids_form: PalwPromptIdsFormV1,
) -> Result<PalwGenJobV1, PalwGenClaimErrorV1> {
    use PalwGenClaimErrorV1 as E;
    if payload.version != crate::palw_freeprompt_v3::PALW_FP_V3_VERSION {
        return Err(E::PayloadVersion { got: payload.version, expected: crate::palw_freeprompt_v3::PALW_FP_V3_VERSION });
    }
    let c = &payload.commitment;
    let job = PalwGenJobV1::from_carried(&c.job)?;
    if let Some(network_domain) = network_domain
        && job.envelope.network_domain != network_domain
    {
        return Err(E::NetworkDomainMismatch);
    }
    match job.envelope.privacy_mode {
        PALW_FP_PRIVACY_PUBLIC_DA => {}
        PALW_FP_PRIVACY_PANEL_DA if panel_da_armed => {}
        PALW_FP_PRIVACY_PANEL_DA => return Err(E::PanelDaNotArmed),
        other => return Err(E::PrivacyModeNotOffered(other)),
    }
    if job.envelope.prompt_mode != PALW_FP_PROMPT_MODE_USER {
        return Err(E::PromptModeNotOffered(job.envelope.prompt_mode));
    }
    if job.envelope.executor_pubkey.is_empty() {
        return Err(E::MissingPublicKey);
    }
    let expected = crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN;
    if payload.signature.len() != expected {
        return Err(E::SignatureLength { got: payload.signature.len(), expected });
    }
    // The body's text commitments: an id list is the zero hash and a count of 0, and nothing else.
    let (prompt_hash, prompt_tokens, negative_hash, negative_tokens) = job.text_commitments();
    for (what, tokens, hash) in [("prompt", prompt_tokens, &prompt_hash), ("negative prompt", negative_tokens, &negative_hash)] {
        if (tokens == 0) != (*hash == Hash64::default()) {
            return Err(E::EmptyIdsEncoding { what, tokens });
        }
    }
    // §I.4.2: the commitment's fields.
    if c.decode_tokens_executed != 0 {
        return Err(E::CommitmentNotCanonical("decode_tokens_executed (0)"));
    }
    if c.stop_reason != PalwFpStopReasonV3::ExactBudgetReached {
        return Err(E::CommitmentNotCanonical("stop_reason (ExactBudgetReached)"));
    }
    if c.schedule_root != Hash64::default() {
        return Err(E::CommitmentNotCanonical("schedule_root (zero)"));
    }
    if c.trace_manifest_root != Hash64::default() || c.trace_chunk_count != 1 || c.trace_retention_daa != 0 {
        return Err(E::CommitmentNotCanonical("the data-availability trio (no manifest, one chunk, no retention)"));
    }
    if c.work_leaves == 0 {
        return Err(E::ZeroWorkLeaves);
    }
    if c.work_leaves > max_step_leaf_count {
        return Err(E::WorkLeavesAboveCap { got: c.work_leaves, max: max_step_leaf_count });
    }
    for (name, root) in [("trace_root", &c.trace_root), ("output_root", &c.output_root), ("execution_root", &c.execution_root)] {
        if *root == Hash64::default() {
            return Err(E::ZeroRoot(name));
        }
    }
    if palw_gen_commitment_execution_root_v1(c, &job) != c.execution_root {
        return Err(E::ExecutionRootNotItsParts);
    }
    // The ids: under PanelDa none; under PublicDa the prompt's, then the negative prompt's, each against
    // its own hash in the network's form.
    if job.envelope.privacy_mode == PALW_FP_PRIVACY_PANEL_DA {
        if !payload.prompt_token_ids.is_empty() {
            return Err(E::PanelDaPayloadCarriesIds(payload.prompt_token_ids.len()));
        }
        return Ok(job);
    }
    let declared = prompt_tokens as u64 + negative_tokens as u64;
    if payload.prompt_token_ids.len() as u64 != declared {
        return Err(E::IdsCount { got: payload.prompt_token_ids.len(), declared });
    }
    let (prompt, negative) = palw_gen_split_ids_v1(&payload.prompt_token_ids, prompt_tokens);
    if prompt_tokens > 0 && !prompt_token_ids_match_v1(prompt_ids_form, prompt, &prompt_hash) {
        return Err(E::IdsNotTheJobs { what: "prompt" });
    }
    if negative_tokens > 0 && !prompt_token_ids_match_v1(prompt_ids_form, negative, &negative_hash) {
        return Err(E::IdsNotTheJobs { what: "negative prompt" });
    }
    Ok(job)
}

// ---------------------------------------------------------------------------------------------
// The doors
// ---------------------------------------------------------------------------------------------

/// **The isolation door for a tensor claim** (RFC-0003 §I.4.4), height-free as isolation is: the payload
/// decodes as a version-10 payload and its claim's stateless rules hold
/// ([`palw_fp_gen_claim_check_v1`] with no network domain and the structural cap). Only where the ruleset
/// carries `Params::palw_fp_job_v5`; the header-context door decides the height.
pub fn validate_palw_fp_gen_commitment_tx_v1(
    payload: &[u8],
    panel_da_admissible: bool,
    prompt_ids_form: PalwPromptIdsFormV1,
    work_leaves_cap: u64,
) -> Result<(), PalwFpV3Error> {
    let payload: PalwFpCommitmentTxPayloadV3 =
        borsh::from_slice(payload).map_err(|_| PalwFpV3Error::TensorClaim(PalwGenClaimErrorV1::PayloadUndecodable.to_string()))?;
    palw_fp_gen_claim_check_v1(&payload, None, panel_da_admissible, work_leaves_cap, prompt_ids_form)?;
    Ok(())
}

/// **The free-prompt door with tensor claims** (RFC-0003 §I.4.4): a tensor claim's payload goes to
/// [`validate_palw_fp_gen_commitment_tx_v1`] where `gen_door` (the ruleset carries `palw_fp_job_v5`), and
/// every other payload — a tensor claim's too, where the door is shut — to the lane's own door, which
/// refuses version 10 by name (`UnsupportedVersion`), as a build without the fence refuses its bytes. So a
/// build that carries the fence and one that does not agree on every transaction below it.
pub fn validate_palw_fp_commitment_tx_gen_door_v1(
    payload: &[u8],
    panel_da_admissible: bool,
    prompt_ids_form: PalwPromptIdsFormV1,
    work_leaves_cap: u64,
    decode_rules: crate::palw_freeprompt_v3::PalwFpDecodeRulesV1,
    gen_door: bool,
) -> Result<(), PalwFpV3Error> {
    if gen_door && palw_fp_payload_is_gen_v1(payload) {
        return validate_palw_fp_gen_commitment_tx_v1(payload, panel_da_admissible, prompt_ids_form, work_leaves_cap);
    }
    crate::palw_fp_objects_v3::validate_palw_fp_commitment_tx_under_v5(
        payload,
        panel_da_admissible,
        prompt_ids_form,
        work_leaves_cap,
        decode_rules,
    )
}

/// **The header-context half of the tensor door** (RFC-0003 §I.4.4): at the containing block's height,
/// why a tensor claim is refused — below `Params::palw_fp_job_v5` — or `None` (for any other payload and
/// for one the height admits). With the isolation door's height-free answer this makes a build that
/// schedules the fence and one that does not agree on every transaction below it: the one refuses the
/// claim here, the other at its isolation door, and a block carrying it is invalid to both.
pub fn palw_fp_gen_refusal_at_v1(payload: &[u8], fp_job_v5_active: bool) -> Option<&'static str> {
    if palw_fp_payload_is_gen_v1(payload) && !fp_job_v5_active {
        return Some("a tensor claim (FP job version 10) below Params::palw_fp_job_v5");
    }
    None
}

/// **Extract one chain block's tensor claims** (RFC-0003 §I.4.4) — the free-prompt walk's twin for
/// version-10 payloads, which the lane's walk skips as not stateless-admissible
/// (`palw_fp_objects_from_accepted_txs_by_class_v1`). Its arguments and the order of its checks are the FP
/// walk's: the payload decodes as a version-10 payload; its claim's stateless rules hold at the claim's
/// CLASS's ladder under the network's domain and arming ([`palw_fp_gen_claim_check_v1`]); its signature
/// verifies under the key it carries. Total over whatever was accepted: a payload that fails any of them
/// is skipped with its reason, never rejected, so a peer's payload cannot invalidate the block that
/// carried it. The class, the work and the capacity are the fold's. The caller runs it only past
/// `palw_fp_job_v5` over `palw_gen_v1` and appends its objects after the free-prompt walk's.
pub fn palw_fp_gen_objects_from_accepted_txs_v1<'a, V, C>(
    txs: &[Transaction],
    network_domain: Hash64,
    panel_da_armed: bool,
    class_caps: C,
    prompt_ids_form: PalwPromptIdsFormV1,
    verify_mldsa87: V,
) -> PalwFpExtractionV3
where
    V: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
    C: Fn(&Hash64) -> PalwFpClassCapsV1<'a>,
{
    let mut out = PalwFpExtractionV3::default();
    for tx in txs {
        if tx.subnetwork_id != SUBNETWORK_ID_PALW_FP_COMMITMENT || !palw_fp_payload_is_gen_v1(&tx.payload) {
            continue;
        }
        let id = tx.id();
        let payload: PalwFpCommitmentTxPayloadV3 = match borsh::from_slice(&tx.payload) {
            Ok(payload) => payload,
            Err(_) => {
                out.skipped.push((id, "tensor payload does not decode"));
                continue;
            }
        };
        let caps = class_caps(&payload.commitment.job.class_id);
        let Ok(job) = palw_fp_gen_claim_check_v1(&payload, Some(network_domain), panel_da_armed, caps.step_ladder, prompt_ids_form)
        else {
            out.skipped.push((id, "tensor payload is not stateless-admissible"));
            continue;
        };
        if payload.validate_signature_v3(&verify_mldsa87).is_err() {
            out.skipped.push((id, "commitment signature does not verify under the carried key"));
            continue;
        }
        let commitment = &payload.commitment;
        out.objects.push(PalwConsensusObjectV3Carrier {
            carrier: id,
            object: PalwConsensusObjectV2::GenTensorCommitted {
                claim: payload.claim_id(),
                class_id: commitment.job.class_id,
                bond: PalwBondKeyV2(commitment.job.executor_bond),
                executor_pubkey: commitment.job.executor_pubkey.clone(),
                work_leaves: commitment.work_leaves,
                prompt_token_ids: payload.prompt_token_ids.clone(),
                trace_root: commitment.trace_root,
                output_root: commitment.output_root,
                execution_root: commitment.execution_root,
                job_pin: crate::palw_fp_execution_v3::palw_fp_job_pin_v1(commitment),
                job: Box::new(job),
            },
        });
    }
    out
}

/// **A tensor payload as the lane builds it** — what an executor's submit path and the tests assemble:
/// the commitment of `job` over the roots a run committed, the roots in §I.4.2's form, and the signature
/// the caller supplies. `execution_root` is the tensor execution root of the parts.
pub fn palw_gen_payload_v1(
    job: &PalwGenJobV1,
    work_leaves: u64,
    step_root: Hash64,
    output_root: Hash64,
    prompt_token_ids: Vec<u32>,
    signature: Vec<u8>,
) -> PalwFpCommitmentTxPayloadV3 {
    let mut commitment = PalwFreePromptCommitmentV3 {
        job: job.into_carried(),
        trace_root: step_root,
        output_root,
        schedule_root: Hash64::default(),
        execution_root: Hash64::default(),
        decode_tokens_executed: 0,
        stop_reason: PalwFpStopReasonV3::ExactBudgetReached,
        work_leaves,
        trace_manifest_root: Hash64::default(),
        trace_chunk_count: 1,
        trace_retention_daa: 0,
    };
    commitment.execution_root = palw_gen_commitment_execution_root_v1(&commitment, job);
    PalwFpCommitmentTxPayloadV3 {
        version: crate::palw_freeprompt_v3::PALW_FP_V3_VERSION,
        commitment,
        prompt_token_ids,
        signature,
    }
}
