//! **Lane J1 (user journey, read-only): the kernel-route job views on op 227** — the kernel class list, one job's status (pending /
//! claimed / Final / SecurityCertified / settled / defaulted, with its escrow) and one job's result tokens with their certification.
//!
//! **Nothing here is a rule**, and nothing here judges a model: the documents only read the kernel ledger the fold already keeps and
//! the Gate C rows ([`crate::palw_security_certification_v1`]). The node holds no tokenizer: the result is token ids; turning them
//! into text is the client's job with a tokenizer the user supplies (a kernel class binds `tokenizer_or_input_schema_root = 0`, so the
//! chain commits to no tokenizer and a client cannot check one by digest — it says so instead).

use crate::palw_security_certification_v1 as cert;
use crate::palw_state_v2::{PalwChainStateV2, PalwStateParamsV2};
use crate::palw_user_views_v1::PalwUserViewClockV1;
use kaspa_hashes::Hash64;
use misaka_palw_kernel::ledger::{ClaimBodyV1, ClaimRowV1, KernelLedgerV1};
use misaka_palw_kernel::lifecycle::ClaimStateV1;
use serde_json::{Value, json};

/// The documents' schema version.
pub const PALW_USER_JOB_VIEWS_VERSION_V1: u32 = 1;
/// The cap on the classes the list returns.
pub const PALW_USER_KERNEL_CLASS_ROWS_V1: usize = 200;

fn hex(b: &[u8; 64]) -> String {
    Hash64::from_bytes(*b).to_string()
}

fn ledger_of(state: &PalwChainStateV2) -> Option<crate::palw_native_resource_v1::ReservedKernelLedgerV1> {
    state.kernel_route_state_v1()?.ledger().ok()
}

/// `kernelClasses`: every single-program kernel class (capped): id, artifact root, tier, whether its artifact is attested public,
/// token bound, max positions, and the tokenizer commitment (always `null` for a kernel class binding).
pub fn palw_kernel_classes_json_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    clock: &PalwUserViewClockV1,
) -> Option<Value> {
    let ledger = ledger_of(state)?;
    let now = clock.next_daa;
    let rows: Vec<Value> = ledger
        .classes
        .iter()
        .take(PALW_USER_KERNEL_CLASS_ROWS_V1)
        .map(|(id, row)| {
            let class = Hash64::from_bytes(*id);
            let root = row.param_commitments.root();
            let tier = crate::palw_model_tier_v1::palw_model_tier_at_v1(state, params, &class, now);
            let c = crate::palw_model_tier_v1::palw_model_tier_coefficient_permille_v1(params.model_tier_v1(), tier, now);
            json!({
                "classId": class.to_string(),
                "artifactRoot": hex(&root),
                "tier": if tier.is_approved() { "1.0" } else { "0.3" },
                "cPermille": c,
                "attested": ledger.attested_artifacts.contains(&root),
                "tokenBound": row.program.token_bound,
                "maxPositions": row.plan.max_positions,
                "tokenizerCommitment": Value::Null,
            })
        })
        .collect();
    Some(json!({
        "version": PALW_USER_JOB_VIEWS_VERSION_V1,
        "kind": "palwKernelClasses",
        "nowDaa": now,
        "ledgerDaa": ledger.daa,
        "classCount": ledger.classes.len(),
        "truncated": ledger.classes.len() > rows.len(),
        "classes": rows,
        "jobQuote": { "rewardEscrowSompi": ledger.policy.claim_reward, "jobFeeBurnedSompi": ledger.policy.job_fee },
        "notes": ["a kernel class commits to no tokenizer: the client tokenises with a tokenizer the user supplies (--tokenizer)"],
    }))
}

/// The claim answering `job`: the holder, else the latest claim committed against it (a failed holder frees the job).
fn job_claim<'a>(ledger: &'a KernelLedgerV1, job: &[u8; 64]) -> Option<([u8; 64], &'a ClaimRowV1)> {
    if let Some(id) = ledger.job_claims.get(job) {
        return ledger.claims.get(id).map(|r| (*id, r));
    }
    ledger.claims.iter().filter(|(_, r)| r.job_id == *job).max_by_key(|(_, r)| r.committed_daa).map(|(id, r)| (*id, r))
}

fn state_text(s: &ClaimStateV1) -> &'static str {
    match s {
        ClaimStateV1::Committed => "committed",
        ClaimStateV1::ChallengeBound { .. } => "challenge-bound",
        ClaimStateV1::Checking { .. } => "checking",
        ClaimStateV1::ProbabilisticPass { .. } => "probabilistic-pass",
        ClaimStateV1::WindowClosed { .. } => "window-closed",
        ClaimStateV1::Final { .. } => "final",
        ClaimStateV1::Disputed { .. } => "disputed",
        ClaimStateV1::Convicted { .. } => "convicted",
        ClaimStateV1::Unavailable { .. } => "unavailable",
        ClaimStateV1::TimedOut { .. } => "timed-out",
        ClaimStateV1::Challengeable { .. } => "challengeable",
    }
}

/// `kernelJob`: one job's status. `status` is one of `pending` (no claim), `claimed` (a live claim), `final` (Final, Gate C
/// pending), `security-certified` (certified, reward still held), `settled` (certified and paid, escrow spent), `defaulted` (the
/// claim failed — convicted / unavailable / timed out / not certified — and no other claim holds the job). `None`: no such job.
pub fn palw_kernel_job_json_v1(state: &PalwChainStateV2, job: &Hash64, clock: &PalwUserViewClockV1) -> Option<Value> {
    let ledger = ledger_of(state)?;
    let id = job.as_bytes();
    let posted = ledger.jobs.get(&id)?;
    let claim = job_claim(&ledger, &id);
    let escrow = ledger.job_escrows.get(&id);
    let poster = escrow.map(|e| e.poster).or_else(|| ledger.job_posters.get(&id).copied());
    let (status, claim_doc) = match claim {
        None => ("pending", Value::Null),
        Some((cid, row)) => {
            let certified = cert::palw_kernel_claim_security_certified_v1(state, &cid);
            let hold = cert::palw_kernel_claim_reward_hold_v1(state, &cid);
            let cert_row = cert::palw_kernel_claim_certification_row_v1(state, &cid);
            let failed_cert = cert_row.as_ref().and_then(|r| r.outcome).is_some_and(|(o, _)| !o.certifies());
            let status = match &row.life.state {
                ClaimStateV1::Final { .. } if failed_cert => "defaulted",
                ClaimStateV1::Final { .. } if certified && hold.is_none() && escrow.is_none() => "settled",
                ClaimStateV1::Final { .. } if certified => "security-certified",
                ClaimStateV1::Final { .. } => "final",
                ClaimStateV1::Convicted { .. } | ClaimStateV1::Unavailable { .. } | ClaimStateV1::TimedOut { .. } => "defaulted",
                _ => "claimed",
            };
            (
                status,
                json!({
                    "claimId": hex(&cid),
                    "producerBond": hex(&row.producer),
                    "state": state_text(&row.life.state),
                    "committedDaa": row.committed_daa,
                    "convicted": row.convicted,
                    "rewarded": row.rewarded,
                    "securityCertified": certified,
                    "heldRewardSompi": hold.as_ref().map(|h| h.amount),
                }),
            )
        }
    };
    let escrow_doc = match escrow {
        Some(e) => json!({ "state": "reserved", "amountSompi": e.amount, "postedDaa": e.posted_daa }),
        None if status == "settled" || claim.is_some_and(|(_, r)| r.rewarded) => {
            json!({ "state": "paid", "paidSompi": ledger.policy.claim_reward, "amountIsPolicyQuote": true })
        }
        None if ledger.policy.claim_reward == 0 => json!({ "state": "none" }),
        None => json!({ "state": "returned", "refundedSompi": ledger.policy.claim_reward, "amountIsPolicyQuote": true }),
    };
    Some(json!({
        "version": PALW_USER_JOB_VIEWS_VERSION_V1,
        "kind": "palwKernelJob",
        "nowDaa": clock.next_daa,
        "ledgerDaa": ledger.daa,
        "jobId": job.to_string(),
        "classId": hex(&posted.class_binding_id),
        "posterBond": poster.as_ref().map(hex),
        "promptTokens": posted.prompt.len(),
        "maxNewTokens": posted.max_new_tokens,
        "status": status,
        "claim": claim_doc,
        "escrow": escrow_doc,
    }))
}

/// `kernelResult`: one job's generated tokens (the claim's delivered `generated`, the whole answer) and their standing:
/// `provisional` until the claim is SecurityCertified (Gate C), `certified` after, `void` when the claim failed. `None`: no such job.
pub fn palw_kernel_result_json_v1(state: &PalwChainStateV2, job: &Hash64, clock: &PalwUserViewClockV1) -> Option<Value> {
    let ledger = ledger_of(state)?;
    let id = job.as_bytes();
    let posted = ledger.jobs.get(&id)?;
    let Some((cid, row)) = job_claim(&ledger, &id) else {
        return Some(json!({
            "version": PALW_USER_JOB_VIEWS_VERSION_V1, "kind": "palwKernelResult", "nowDaa": clock.next_daa,
            "jobId": job.to_string(), "classId": hex(&posted.class_binding_id), "standing": "none", "tokens": Value::Null,
        }));
    };
    let generated: Option<&Vec<u32>> = match &row.body {
        ClaimBodyV1::Program { claim, .. } => Some(&claim.generated),
        _ => None,
    };
    let certified = cert::palw_kernel_claim_security_certified_v1(state, &cid);
    let failed = row.convicted
        || matches!(row.life.state, ClaimStateV1::Convicted { .. } | ClaimStateV1::Unavailable { .. } | ClaimStateV1::TimedOut { .. })
        || cert::palw_kernel_claim_certification_row_v1(state, &cid).and_then(|r| r.outcome).is_some_and(|(o, _)| !o.certifies());
    let standing = if failed {
        "void"
    } else if certified {
        "certified"
    } else {
        "provisional"
    };
    Some(json!({
        "version": PALW_USER_JOB_VIEWS_VERSION_V1,
        "kind": "palwKernelResult",
        "nowDaa": clock.next_daa,
        "jobId": job.to_string(),
        "classId": hex(&posted.class_binding_id),
        "claimId": hex(&cid),
        "claimState": state_text(&row.life.state),
        "securityCertified": certified,
        "standing": standing,
        "tokens": generated,
        "tokenizerCommitment": Value::Null,
        "notes": ["token ids only: the node holds no tokenizer; `misaka palw job result --tokenizer <file>` renders text"],
    }))
}
