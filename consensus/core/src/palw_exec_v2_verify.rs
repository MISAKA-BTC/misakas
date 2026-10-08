//! RFC-0008 v2, **amendment 1 (spec §10.1): the slice verification route is the G14 kernel route.**
//!
//! A work slice is verified, convicted or defaulted only through a claim of the probabilistic-constraint kernel route
//! (`palw_probabilistic_constraints_v1`, object tag 110): the route the repository's G14 cases prove on the real node — one ordinary
//! outside bond, from public material alone, convicts a covered lie or defaults withheld material, and an OPV class finalizes after its
//! public window with no Panel. This module holds the pure half of the binding:
//!
//! * the **token state** a slice's boundaries commit to ([`palw_exec_v2_token_state_root_v1`]): a slice is a run of generated tokens
//!   continuing a prompt, so its predecessor is the state of the prompt, its result the state of the prompt and the run;
//! * the **slice job nonce** ([`palw_exec_v2_slice_job_nonce_v1`]): the kernel job that backs slice `(root, index, range)` carries it, so
//!   one kernel job — hence one claim, the route holding a job for its first live claim — backs exactly one slice;
//! * the **binding check** ([`palw_exec_v2_verification_binding_v1`]): the kernel claim the slice names (`slice.evidence_root`) is a
//!   program claim of the root class's bound kernel class, by the slice's executor, of the slice's job, whose prompt, run and evidence
//!   are the slice's predecessor, result, output and DA roots, and that has not failed;
//! * the **outcome** a kernel claim's row says for its slice ([`palw_exec_v2_claim_outcome_v1`]), and the **leg cap** a verified slice
//!   takes from it ([`palw_exec_v2_leg_cap_v1`]).
//!
//! The fold (`palw_exec_v2_fold`) reads the rows (the route's tables, the onboarding binding) and applies the outcome; nothing here
//! reads state. Integer and hash only.

use crate::Hash64;
use crate::palw_exec_v2::PalwWorkSliceV1;
use crate::palw_onboarding_v1::KernelBindingRowV1;
use crate::palw_work_slice_v2::PalwSliceRefusalV2;
use misaka_palw_kernel::job::KernelJobV1;
use misaka_palw_kernel::ledger::{ClaimBodyV1, ClaimRowV1};
use misaka_palw_kernel::lifecycle::ClaimStateV1;

/// The domain of the slice job nonce.
pub const PALW_EXEC_V2_SLICE_JOB_NONCE_DOMAIN_V1: &[u8] = b"misaka-palw/exec-v2/slice-job-nonce/v1";
/// The domain of a token state.
pub const PALW_EXEC_V2_TOKEN_STATE_DOMAIN_V1: &[u8] = b"misaka-palw/exec-v2/token-state/v1";

fn keyed(domain: &[u8]) -> blake2b_simd::State {
    blake2b_simd::Params::new().hash_length(64).key(domain).to_state()
}

fn finish(state: blake2b_simd::State) -> Hash64 {
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// **The token state of a token run** — `H(domain; n ‖ t_0 ‖ … ‖ t_{n-1})` (`n` u64 LE, each token u32 LE) over the concatenation of
/// `parts`, so `token_state(&[a, b]) == token_state(&[a ++ b])`: a slice's result (`prompt ‖ run`) is the next slice's predecessor
/// (its prompt) exactly when the next prompt continues the stream.
pub fn palw_exec_v2_token_state_root_v1(parts: &[&[u32]]) -> Hash64 {
    let mut state = keyed(PALW_EXEC_V2_TOKEN_STATE_DOMAIN_V1);
    let n: u64 = parts.iter().map(|part| part.len() as u64).sum();
    state.update(&n.to_le_bytes());
    for part in parts {
        for token in part.iter() {
            state.update(&token.to_le_bytes());
        }
    }
    finish(state)
}

/// **The nonce of the kernel job that backs a slice**: `H(domain; root claim ‖ index ‖ range ‖ canonical job ‖ plan root)`. The job's id
/// hashes its nonce, so a job carrying it is this slice's and no other's.
pub fn palw_exec_v2_slice_job_nonce_v1(slice: &PalwWorkSliceV1) -> Hash64 {
    let mut state = keyed(PALW_EXEC_V2_SLICE_JOB_NONCE_DOMAIN_V1);
    state.update(slice.root_claim_id.as_byte_slice());
    state.update(&slice.slice_index.to_le_bytes());
    state.update(&slice.canonical_range.start.to_le_bytes());
    state.update(&slice.canonical_range.end.to_le_bytes());
    state.update(slice.canonical_job_id.as_byte_slice());
    state.update(slice.plan_root.as_byte_slice());
    finish(state)
}

/// **What a kernel claim's row says of its slice.** Read every block the row changes (and once at admission).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwSliceVerificationV1 {
    /// The claim is live and undecided (committed, checking, challengeable, disputed …).
    Pending,
    /// The claim is Final: the slice is positively verified.
    Verified { final_daa: u64 },
    /// The claim was convicted — before or after its Final: the slice is proven false.
    ProvenFalse,
    /// The claim is unavailable (withheld material, a default) or timed out: the slice can never be verified.
    Defaulted,
}

/// The outcome of a kernel claim row at `daa` (a conviction wins over a Final: the route flips `convicted` on a lie convicted inside
/// its liability horizon).
///
/// **A Final claim that forfeited its reservation inside its liability horizon is a default** (the X8R round-2 review): the route
/// answers a demand the producer leaves unserved after `Final` by taking the WHOLE remaining reservation and leaving the state `Final`
/// (`PostFinalDefault`), so the state alone would still say "verified" of a claim whose material was withheld. Inside the horizon the
/// route holds a Final claim's reservation in full — it is released only once the horizon has passed (`daa > liability_until`) — so a
/// Final row reserving nothing at or before its horizon is exactly that forfeit. Past the horizon a released row is still verified.
pub fn palw_exec_v2_claim_outcome_v1(row: &ClaimRowV1, daa: u64) -> PalwSliceVerificationV1 {
    if row.convicted {
        return PalwSliceVerificationV1::ProvenFalse;
    }
    match row.life.state {
        ClaimStateV1::Final { .. } if row.reserved == 0 && row.liability_until.is_none_or(|until| daa <= until) => {
            PalwSliceVerificationV1::Defaulted
        }
        ClaimStateV1::Final { final_daa } => PalwSliceVerificationV1::Verified { final_daa },
        ClaimStateV1::Convicted { .. } => PalwSliceVerificationV1::ProvenFalse,
        ClaimStateV1::Unavailable { .. } | ClaimStateV1::TimedOut { .. } => PalwSliceVerificationV1::Defaulted,
        _ => PalwSliceVerificationV1::Pending,
    }
}

/// **The most a verified slice's executor may be paid for it at the root's settlement** (spec §10.5, as revised by the X8R round-2
/// review): the reservation its kernel claim holds when the slice verifies, less the `Final` reward the route paid on that same claim.
///
/// * **Why net of the route's reward.** The route's own invariant is `claim_reward < claim_collateral` ("the Final reward is smaller
///   than the reservation a post-Final default forfeits"): what one claim can gain stays below what it puts at risk. A slice leg is a
///   second gain on the same claim, so the two together are held under the same reservation.
/// * **Why fixed at verification.** Read at settlement instead (the first amendment), the cap fell to zero for every slice whose
///   claim's liability horizon had passed — the route releases the reservation then — so a root bond that held back the LAST slice
///   until the earlier executors' horizons passed kept their whole legs. The liability the cap stands for is the one that existed while
///   the claim could still be convicted; its later release changes neither the risk nor the work.
pub fn palw_exec_v2_leg_cap_v1(row: &ClaimRowV1, claim_reward: u64) -> u64 {
    let reward = if row.rewarded { claim_reward } else { 0 };
    row.reserved.saturating_sub(reward)
}

/// **The verification binding** (admission rule 5, amendment 1): does the kernel claim `row` (the claim `slice.evidence_root` names, its
/// job `job` as the route holds it) verify exactly this slice, for the root class's kernel `binding`, executed by the bond whose kernel
/// digest is `executor_kernel_bond`, judged at `daa`? Each mismatch is named; nothing is written by a caller on a refusal.
pub fn palw_exec_v2_verification_binding_v1(
    slice: &PalwWorkSliceV1,
    binding: &KernelBindingRowV1,
    row: &ClaimRowV1,
    job: Option<&KernelJobV1>,
    executor_kernel_bond: &[u8; 64],
    daa: u64,
) -> Result<(), PalwSliceRefusalV2> {
    use PalwSliceRefusalV2::{VerificationClaimFailed, VerificationClaimNotBound, VerificationKindUnsupported};
    let ClaimBodyV1::Program { claim, .. } = &row.body else {
        return Err(VerificationKindUnsupported);
    };
    if matches!(palw_exec_v2_claim_outcome_v1(row, daa), PalwSliceVerificationV1::ProvenFalse | PalwSliceVerificationV1::Defaulted) {
        return Err(VerificationClaimFailed);
    }
    if Hash64::from_bytes(row.class_binding_id) != binding.kernel_class {
        return Err(VerificationClaimNotBound("class"));
    }
    if row.producer != *executor_kernel_bond || claim.producer_bond != *executor_kernel_bond {
        return Err(VerificationClaimNotBound("executor"));
    }
    let job = job.ok_or(VerificationClaimNotBound("job"))?;
    if claim.job_id != row.job_id || job.id() != row.job_id || job.class_binding_id != row.class_binding_id {
        return Err(VerificationClaimNotBound("job"));
    }
    if Hash64::from_bytes(job.nonce) != palw_exec_v2_slice_job_nonce_v1(slice) {
        return Err(VerificationClaimNotBound("job nonce"));
    }
    if slice.predecessor_state_root != palw_exec_v2_token_state_root_v1(&[&job.prompt]) {
        return Err(VerificationClaimNotBound("predecessor"));
    }
    if slice.result_state_root != palw_exec_v2_token_state_root_v1(&[&job.prompt, &claim.generated]) {
        return Err(VerificationClaimNotBound("result"));
    }
    if slice.output_root != palw_exec_v2_token_state_root_v1(&[&claim.generated]) {
        return Err(VerificationClaimNotBound("output"));
    }
    if slice.da_root != Hash64::from_bytes(claim.evidence_root) {
        return Err(VerificationClaimNotBound("evidence"));
    }
    Ok(())
}

/// The key of a kernel route row named by a 64-byte id (the route keys its tables by the borsh of the digest).
pub fn palw_exec_v2_kernel_key_v1(id: &Hash64) -> Vec<u8> {
    borsh::to_vec(&id.as_bytes()).expect("a digest serializes")
}

// ---------------------------------------------------------------------------------------------
// The producer's half: a slice statement from public rows
// ---------------------------------------------------------------------------------------------

/// **The slice statement `bond` should sign for `(root, index)`, backed by the kernel claim `claim_id`** — derived entirely from the
/// tip state (the root's plan and boundary, the kernel route's claim and job rows), so the executor's node publishes exactly what the
/// fold's binding (amendment 1) will accept: the plan's range, the root's bindings, the token states of the claim's prompt and run, the
/// claim id as `evidence_root` and the claim's own evidence root as `da_root`. Refused by name when the root is not open, the index is
/// not the next one, the bond is not authorised, or the claim does not exist or is not a program claim of this bond. The fold judges the
/// carrier again; this only spares the executor a carrier it would refuse.
pub fn palw_exec_v2_slice_statement_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    root_claim_id: Hash64,
    index: u32,
    claim_id: Hash64,
    bond: crate::palw_state_v2::PalwBondKeyV2,
) -> Result<PalwWorkSliceV1, String> {
    use crate::palw_work_slice_v2::{PalwWorkRootPhaseV2, palw_work_plan_range_v2};
    let root = state.exec_v2_root_v1(&root_claim_id).ok_or("no such root on this node's chain")?;
    if !matches!(root.phase, PalwWorkRootPhaseV2::Open) {
        return Err(format!("the root is not open ({:?})", root.phase));
    }
    if index != root.next_index {
        return Err(format!("slice {index} is not the next one ({})", root.next_index));
    }
    if !root.authorises(&bond) {
        return Err("this bond is not an authorised executor of the root".into());
    }
    let canonical_range = palw_work_plan_range_v2(&root.boundaries, index).ok_or("the index is past the plan")?;
    let route = state.kernel_route().ok_or("no kernel route on this chain: a slice has no verification claim")?;
    let row = route
        .rows
        .get(&(misaka_palw_kernel::rows::TABLE_CLAIMS_V1, palw_exec_v2_kernel_key_v1(&claim_id)))
        .and_then(|bytes| borsh::from_slice::<ClaimRowV1>(bytes).ok())
        .ok_or("the kernel route holds no such claim")?;
    let ClaimBodyV1::Program { claim, .. } = &row.body else {
        return Err("the claim is a pipeline claim: pipeline-class slices await a segment state".into());
    };
    if row.producer != crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(&bond) {
        return Err("the claim is another bond's".into());
    }
    let job = route
        .rows
        .get(&(misaka_palw_kernel::rows::TABLE_JOBS_V1, borsh::to_vec(&row.job_id).expect("a digest serializes")))
        .and_then(|bytes| borsh::from_slice::<KernelJobV1>(bytes).ok())
        .ok_or("the kernel route holds no job for the claim")?;
    Ok(PalwWorkSliceV1 {
        root_claim_id,
        slice_index: index,
        class_id: root.class_id,
        canonical_job_id: root.canonical_job_id,
        kernel_version: root.kernel_version,
        plan_root: root.plan_root,
        canonical_range,
        predecessor_state_root: palw_exec_v2_token_state_root_v1(&[&job.prompt]),
        result_state_root: palw_exec_v2_token_state_root_v1(&[&job.prompt, &claim.generated]),
        input_root: root.initial_state_root,
        output_root: palw_exec_v2_token_state_root_v1(&[&claim.generated]),
        evidence_root: claim_id,
        da_root: Hash64::from_bytes(claim.evidence_root),
        executor_bond: bond,
    })
}

// ---------------------------------------------------------------------------------------------
// The read model (RPC op 240): a versioned observation, read by no rule
// ---------------------------------------------------------------------------------------------

/// The observation's own version: fields are only ever appended.
pub const PALW_EXEC_V2_OBSERVATION_VERSION_V1: u32 = 1;
/// The most roots one observation names.
pub const PALW_EXEC_V2_OBSERVATION_MAX_ROOTS_V1: usize = 64;

/// **`getPalwExecV2Status` (op 240)**: the EXEC v2 lane at the tip — counts, the named (or first) roots with every slice and the state
/// of its verification claim, a block's refusal record, and the lane's health. A read model: no rule reads it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PalwExecV2ObservationV1 {
    pub version: u32,
    pub fence_daa: Option<u64>,
    pub active: bool,
    pub tip: String,
    pub tip_daa: u64,
    pub roots_total: usize,
    pub slices_total: usize,
    pub jobs_total: usize,
    pub anchored_total: usize,
    pub roots: Vec<PalwExecV2RootObservationV1>,
    pub unknown_roots: Vec<String>,
    pub block: Option<PalwExecV2BlockObservationV1>,
    pub lane: Option<PalwExecV2LaneObservationV1>,
    /// What a `Verified` slice means — never shown as a proof of physical execution.
    pub statement: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PalwExecV2RootObservationV1 {
    pub root_claim_id: String,
    pub phase: String,
    pub class_id: String,
    pub kernel_class: Option<String>,
    pub plan_root: String,
    pub total_work: u64,
    pub prefix_work: u64,
    pub slice_count: u32,
    pub next_index: u32,
    pub accepted_work: u64,
    pub verified_work: u64,
    pub pending: u32,
    pub ready_for_final: bool,
    pub expiry_daa: u64,
    pub root_bond: String,
    pub extra_executors: Vec<String>,
    pub slices: Vec<PalwExecV2SliceObservationV1>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PalwExecV2SliceObservationV1 {
    pub index: u32,
    pub stage: String,
    pub range_start: u64,
    pub range_end: u64,
    pub executor: String,
    pub carrier: String,
    pub accepted_daa: u64,
    /// The kernel claim that verifies it (`evidence_root`), and that claim's lifecycle as the route holds it.
    pub verification_claim: String,
    pub verification_state: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PalwExecV2BlockObservationV1 {
    pub block: String,
    /// One per covered `EXEC_SLICE` carrier, in the fold's order: code 0 admitted, else the refusal's code and name.
    pub verdicts: Vec<PalwExecV2VerdictObservationV1>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PalwExecV2VerdictObservationV1 {
    pub carrier: String,
    pub code: u8,
    pub name: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PalwExecV2LaneObservationV1 {
    pub latest_head: Option<String>,
    pub pending: u64,
    pub stale: bool,
    pub stale_reason: Option<&'static str>,
}

impl PalwExecV2ObservationV1 {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("the observation serializes")
    }
}

fn bond_name(bond: &crate::palw_state_v2::PalwBondKeyV2) -> String {
    format!("{}:{}", bond.0.transaction_id, bond.0.index)
}

/// The statement a `Verified` slice carries.
pub const PALW_EXEC_V2_VERIFIED_STATEMENT_V1: &str = "a Verified slice is one whose kernel-route claim reached Final with no conviction or \
default; it is not a proof of physical execution time";

/// **Build the observation** over `state` (the tip, `tip` at `tip_daa`) for the named roots (or the first open ones), a block's refusal
/// record (`verdicts`, read from that block's delta) and the lane's health. Pure.
#[allow(clippy::too_many_arguments)]
pub fn palw_exec_v2_observation_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    fence_daa: Option<u64>,
    tip: Hash64,
    tip_daa: u64,
    roots: &[Hash64],
    block: Option<(Hash64, Vec<(Hash64, u8)>)>,
    lane: Option<crate::palw_exec_v2_anchor::PalwExecV2LaneHealthV1>,
) -> PalwExecV2ObservationV1 {
    let table = state.exec_v2_state_v1();
    let route = state.kernel_route();
    let named: Vec<Hash64> = if roots.is_empty() {
        table.roots.keys().take(PALW_EXEC_V2_OBSERVATION_MAX_ROOTS_V1).copied().collect()
    } else {
        roots.iter().take(PALW_EXEC_V2_OBSERVATION_MAX_ROOTS_V1).copied().collect()
    };
    let mut observed = Vec::new();
    let mut unknown = Vec::new();
    for id in named {
        let Some(root) = table.roots.get(&id) else {
            unknown.push(id.to_string());
            continue;
        };
        let slices = table
            .slices
            .range((id, 0)..=(id, u32::MAX))
            .map(|((_, index), row)| PalwExecV2SliceObservationV1 {
                index: *index,
                stage: format!("{:?}", row.stage),
                range_start: row.range.start,
                range_end: row.range.end,
                executor: bond_name(&row.executor),
                carrier: row.carrier.to_string(),
                accepted_daa: row.accepted_daa,
                verification_claim: row.evidence_root.to_string(),
                verification_state: route
                    .and_then(|r| {
                        r.rows.get(&(misaka_palw_kernel::rows::TABLE_CLAIMS_V1, palw_exec_v2_kernel_key_v1(&row.evidence_root)))
                    })
                    .and_then(|bytes| borsh::from_slice::<ClaimRowV1>(bytes).ok())
                    .map(|claim| match (claim.convicted, &claim.life.state, palw_exec_v2_claim_outcome_v1(&claim, tip_daa)) {
                        (true, _, _) => "Convicted".to_string(),
                        (false, ClaimStateV1::Final { .. }, PalwSliceVerificationV1::Defaulted) => "ForfeitedAfterFinal".to_string(),
                        (false, state, _) => format!("{state:?}"),
                    }),
            })
            .collect();
        observed.push(PalwExecV2RootObservationV1 {
            root_claim_id: id.to_string(),
            phase: format!("{:?}", root.phase),
            class_id: root.class_id.to_string(),
            kernel_class: route.and_then(|r| r.kernel_binding_v1(&root.class_id)).map(|binding| binding.kernel_class.to_string()),
            plan_root: root.plan_root.to_string(),
            total_work: root.total_work,
            prefix_work: root.prefix_work(),
            slice_count: root.slice_count(),
            next_index: root.next_index,
            accepted_work: root.accepted_work,
            verified_work: root.verified_work,
            pending: root.pending,
            ready_for_final: root.ready_for_final(),
            expiry_daa: root.expiry_daa,
            root_bond: bond_name(&root.root_bond),
            extra_executors: root.extra_executors.iter().map(bond_name).collect(),
            slices,
        });
    }
    PalwExecV2ObservationV1 {
        version: PALW_EXEC_V2_OBSERVATION_VERSION_V1,
        fence_daa,
        active: fence_daa.is_some_and(|fence| tip_daa >= fence),
        tip: tip.to_string(),
        tip_daa,
        roots_total: table.roots.len(),
        slices_total: table.slices.len(),
        jobs_total: table.jobs.len(),
        anchored_total: table.anchored.len(),
        roots: observed,
        unknown_roots: unknown,
        block: block.map(|(hash, verdicts)| PalwExecV2BlockObservationV1 {
            block: hash.to_string(),
            verdicts: verdicts
                .into_iter()
                .map(|(carrier, code)| PalwExecV2VerdictObservationV1 {
                    carrier: carrier.to_string(),
                    code,
                    name: PalwSliceRefusalV2::name_of_code(code),
                })
                .collect(),
        }),
        lane: lane.map(|health| PalwExecV2LaneObservationV1 {
            latest_head: health.latest_head.map(|head| head.to_string()),
            pending: health.pending,
            stale: health.stale,
            stale_reason: health.stale_reason.map(|reason| reason.as_str()),
        }),
        statement: PALW_EXEC_V2_VERIFIED_STATEMENT_V1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_state_is_one_function_of_the_stream_whatever_the_split() {
        let whole = palw_exec_v2_token_state_root_v1(&[&[1, 2, 3, 4, 5]]);
        assert_eq!(palw_exec_v2_token_state_root_v1(&[&[1, 2], &[3, 4, 5]]), whole);
        assert_eq!(palw_exec_v2_token_state_root_v1(&[&[1, 2, 3, 4, 5], &[]]), whole);
        assert_ne!(palw_exec_v2_token_state_root_v1(&[&[1, 2, 3, 4]]), whole, "a shorter stream");
        assert_ne!(palw_exec_v2_token_state_root_v1(&[&[1, 2, 3, 5, 4]]), whole, "another order");
        assert_ne!(palw_exec_v2_token_state_root_v1(&[]), palw_exec_v2_token_state_root_v1(&[&[0]]), "the length is bound");
    }

    fn slice() -> PalwWorkSliceV1 {
        let h = Hash64::from_u64_word;
        PalwWorkSliceV1 {
            root_claim_id: h(100),
            slice_index: 2,
            class_id: h(101),
            canonical_job_id: h(102),
            kernel_version: 3,
            plan_root: h(103),
            canonical_range: crate::palw_exec_v2::PalwWorkRangeV1 { start: 100, end: 200 },
            predecessor_state_root: h(104),
            result_state_root: h(105),
            input_root: h(106),
            output_root: h(107),
            evidence_root: h(108),
            da_root: h(109),
            executor_bond: crate::palw_state_v2::PalwBondKeyV2(crate::tx::TransactionOutpoint::new(
                crate::tx::TransactionId::from_u64_word(7),
                0,
            )),
        }
    }

    #[test]
    fn the_slice_job_nonce_moves_with_every_field_it_binds_and_no_other() {
        let base = slice();
        let nonce = palw_exec_v2_slice_job_nonce_v1(&base);
        let mut other = base.clone();
        other.slice_index = 3;
        assert_ne!(palw_exec_v2_slice_job_nonce_v1(&other), nonce, "index");
        let mut other = base.clone();
        other.canonical_range.end = 201;
        assert_ne!(palw_exec_v2_slice_job_nonce_v1(&other), nonce, "range");
        let mut other = base.clone();
        other.root_claim_id = Hash64::from_u64_word(0xEE);
        assert_ne!(palw_exec_v2_slice_job_nonce_v1(&other), nonce, "root");
        let mut other = base.clone();
        other.canonical_job_id = Hash64::from_u64_word(0xEF);
        assert_ne!(palw_exec_v2_slice_job_nonce_v1(&other), nonce, "job");
        let mut other = base.clone();
        other.plan_root = Hash64::from_u64_word(0xF0);
        assert_ne!(palw_exec_v2_slice_job_nonce_v1(&other), nonce, "plan");
        // The executor, the results and the evidence are the claim's to bind, not the job's.
        let mut other = base.clone();
        other.result_state_root = Hash64::from_u64_word(0xF1);
        other.evidence_root = Hash64::from_u64_word(0xF2);
        assert_eq!(palw_exec_v2_slice_job_nonce_v1(&other), nonce);
    }
}
