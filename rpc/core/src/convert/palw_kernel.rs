//!
//! **The kernel route's public reads (ops 210, 211, 212) and the onboarding conformance read (op 231): one builder per op, from a
//! node's [`ConsensusApi`] to the op's response.**
//!
//! The RPC service's handlers parse the request, check the network is ConsensusV2, and then call these builders and nothing else.
//! The real-node G14 harness (lane G14C, `consensus/src/pipeline/virtual_processor/tests/g14_kernel_route_e2e/canonical.rs`) calls the
//! same parsers and builders against the node it runs, and then carries each response through the RPC's JSON wire form. So the
//! fresh verifier there is built from exactly the bytes these ops serve, not from a direct read of consensus state.
//!
//! Every request is parsed before a byte of chain state is read: a malformed request is an error on every network, never an
//! absence.

use crate::{
    GetPalwConformanceEvidenceRequest, GetPalwConformanceEvidenceResponse, GetPalwKernelClaimRequest, GetPalwKernelClaimResponse,
    GetPalwKernelFinalsRequest, GetPalwKernelFinalsResponse, GetPalwKernelRowsRequest, GetPalwKernelRowsResponse, RpcError,
    RpcPalwKernelDemand, RpcPalwKernelFinal, RpcPalwKernelRow, RpcPalwKernelSeat, RpcPalwKernelServed, RpcResult,
};
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_hashes::Hash64;

/// `getPalwKernelFinals`'s default page of Finals and the most a caller may ask.
pub const PALW_KERNEL_FINALS_RPC_ROWS: usize = 64;
pub const PALW_KERNEL_FINALS_RPC_ROWS_MAX: usize = 1024;
/// `getPalwKernelRows`'s default page budget (key and row bytes) and the most a caller may ask.
pub const PALW_KERNEL_ROWS_RPC_BYTES: usize = 1 << 20;
pub const PALW_KERNEL_ROWS_RPC_BYTES_MAX: usize = 8 << 20;

/// A 128-hex `Hash64` off the wire, or an error naming the field.
fn parse_hash64(text: &str, what: &str) -> RpcResult<Hash64> {
    text.parse::<Hash64>().map_err(|_| RpcError::General(format!("{what} '{text}' is not a 128-hex Hash64")))
}

fn hex(bytes: &[u8]) -> String {
    faster_hex::hex_string(bytes)
}

// ---- op 210 `getPalwKernelClaim` -------------------------------------------------------------------------------------------------

/// The claim id op 210 asks about.
pub fn palw_kernel_claim_request_v1(request: &GetPalwKernelClaimRequest) -> RpcResult<Hash64> {
    parse_hash64(request.claim_id.trim(), "claim id")
}

/// Op 210's answer on a ConsensusV2 network.
pub fn palw_kernel_claim_response_v1(c: &dyn ConsensusApi, claim: Hash64) -> RpcResult<GetPalwKernelClaimResponse> {
    let tip_daa = c.get_virtual_daa_score();
    let kernel_claim = claim.as_bytes();
    let Some(read) = c.palw_kernel_route_v1().map(|route| route.claim_read_v1(&kernel_claim)) else {
        return Ok(GetPalwKernelClaimResponse { claim_id: claim.to_string(), tip_daa, ..Default::default() });
    };
    let Some(read) = read.map_err(RpcError::General)? else {
        return Ok(GetPalwKernelClaimResponse { available: true, claim_id: claim.to_string(), tip_daa, ..Default::default() });
    };
    Ok(GetPalwKernelClaimResponse {
        available: true,
        found: true,
        tip_daa,
        claim_id: claim.to_string(),
        kind: read.kind.to_string(),
        state: read.state,
        final_daa: read.final_daa.unwrap_or(0),
        convicted: read.convicted,
        rewarded: read.rewarded,
        reserved_sompi: read.reserved,
        committed_daa: read.committed_daa,
        liability_until: read.liability_until.unwrap_or(0),
        producer_bond: hex(&read.producer_bond),
        job_id: hex(&read.job_id),
        class_id: hex(&read.class_id),
        public_record: hex(&read.public_record),
        record_header: hex(&read.record_header),
        served: read
            .served
            .iter()
            .map(|(stage, position, bytes)| RpcPalwKernelServed { stage: *stage as u32, position: *position, bytes: hex(bytes) })
            .collect(),
        demands: read
            .demands
            .iter()
            .map(|d| RpcPalwKernelDemand {
                stage: d.stage as u32,
                position: d.position,
                demanders: d.demanders,
                filed_daa: d.filed_daa,
                deadline_daa: d.deadline_daa,
                last_rejection: d.last_rejection.clone().unwrap_or_default(),
            })
            .collect(),
        seats: read
            .seats
            .iter()
            .map(|(bond, kernel_bond)| RpcPalwKernelSeat {
                bond: format!("{}:{}", bond.0.transaction_id, bond.0.index),
                kernel_bond: hex(kernel_bond),
            })
            .collect(),
        quorum: read.quorum as u32,
        assignment_deadline_daa: read.assignment_deadline_daa,
        receipts_counted: read.receipts_counted,
        ledger_root: read.ledger_root.to_string(),
        aux_root: read.aux_root.to_string(),
        mode: read.mode.to_string(),
        opv: read.opv.is_some(),
        opv_admitted_daa: read.opv.as_ref().map(|o| o.admitted_daa).unwrap_or(0),
        opv_verifier_start_cutoff_daa: read.opv.as_ref().map(|o| o.verifier_start_cutoff_daa).unwrap_or(0),
        opv_final_floor_daa: read.opv.as_ref().map(|o| o.final_floor_daa).unwrap_or(0),
        opv_hard_deadline_daa: read.opv.as_ref().map(|o| o.hard_deadline_daa).unwrap_or(0),
        opv_reservation_sompi: read.opv.as_ref().map(|o| o.reservation).unwrap_or(0),
        opv_max_gain_sompi: read.opv.as_ref().map(|o| o.max_gain).unwrap_or(0),
        final_statement: read.opv.as_ref().map(|o| o.statement.to_string()).unwrap_or_default(),
    })
}

// ---- op 211 `getPalwKernelRows` --------------------------------------------------------------------------------------------------

/// Op 211's parsed request: the cursor to resume after, and the page's byte budget.
pub fn palw_kernel_rows_request_v1(request: &GetPalwKernelRowsRequest) -> RpcResult<(Option<(u8, Vec<u8>)>, usize)> {
    let after = if request.has_cursor {
        let table = u8::try_from(request.after_table).map_err(|_| RpcError::General("afterTable must fit a u8".to_string()))?;
        let text = request.after_key.trim();
        if text.len() % 2 != 0 {
            return Err(RpcError::General("afterKey must be an even number of hex digits".to_string()));
        }
        let mut key = vec![0u8; text.len() / 2];
        faster_hex::hex_decode(text.as_bytes(), &mut key).map_err(|e| RpcError::General(format!("afterKey is not hex: {e}")))?;
        Some((table, key))
    } else {
        None
    };
    let max_bytes = match request.max_bytes {
        0 => PALW_KERNEL_ROWS_RPC_BYTES,
        n => (n as usize).min(PALW_KERNEL_ROWS_RPC_BYTES_MAX),
    };
    Ok((after, max_bytes))
}

/// Op 211's answer on a ConsensusV2 network.
pub fn palw_kernel_rows_response_v1(
    c: &dyn ConsensusApi,
    after: Option<(u8, Vec<u8>)>,
    max_bytes: usize,
) -> RpcResult<GetPalwKernelRowsResponse> {
    let tip_daa = c.get_virtual_daa_score();
    let Some(route) = c.palw_kernel_route_v1() else {
        return Ok(GetPalwKernelRowsResponse { tip_daa, ..Default::default() });
    };
    let page = route.rows_page_v1(after, max_bytes);
    let header = borsh::to_vec(&route.header).map_err(|e| RpcError::General(e.to_string()))?;
    let (more, next_table, next_key) = match &page.next {
        Some((table, key)) => (true, *table as u32, hex(key)),
        None => (false, 0, String::new()),
    };
    Ok(GetPalwKernelRowsResponse {
        available: true,
        tip_daa,
        ledger_root: route.ledger_root().to_string(),
        aux_root: route.aux_root().to_string(),
        header: hex(&header),
        rows: page
            .rows
            .iter()
            .map(|(table, key, row)| RpcPalwKernelRow { table: *table as u32, key: hex(key), row: hex(row) })
            .collect(),
        more,
        next_table,
        next_key,
        total_rows: page.total_rows,
    })
}

// ---- op 212 `getPalwKernelFinals` ------------------------------------------------------------------------------------------------

/// Op 212's page size.
pub fn palw_kernel_finals_request_v1(request: &GetPalwKernelFinalsRequest) -> usize {
    match request.limit {
        0 => PALW_KERNEL_FINALS_RPC_ROWS,
        n => (n as usize).min(PALW_KERNEL_FINALS_RPC_ROWS_MAX),
    }
}

/// Op 212's answer on a ConsensusV2 network: the newest Finals first (a beacon reads the latest).
pub fn palw_kernel_finals_response_v1(c: &dyn ConsensusApi, limit: usize) -> RpcResult<GetPalwKernelFinalsResponse> {
    let tip_daa = c.get_virtual_daa_score();
    let Some(route) = c.palw_kernel_route_v1() else {
        return Ok(GetPalwKernelFinalsResponse { tip_daa, ..Default::default() });
    };
    let ledger_root = route.ledger_root();
    let finals = route.finals_read_v1().map_err(RpcError::General)?;
    let total = finals.len() as u64;
    let rows = finals
        .iter()
        .rev()
        .take(limit)
        .map(|f| RpcPalwKernelFinal {
            claim_id: hex(&f.receipt.claim),
            mode: f.receipt.mode.name().to_string(),
            final_path: f.final_path.to_string(),
            source_profile_id: hex(&f.receipt.source_profile_id),
            canonical_work_id: hex(&f.receipt.canonical_work_id),
            execution_commitment: hex(&f.receipt.execution_commitment),
            accepted_daa: f.receipt.accepted_daa,
            final_daa: f.receipt.final_daa,
            da_satisfied: f.receipt.da_satisfied,
            standing: format!("{:?}", f.receipt.standing),
            work_final_event: f.event.as_deref().map(hex).unwrap_or_default(),
            statement: f.statement.to_string(),
        })
        .collect();
    Ok(GetPalwKernelFinalsResponse { available: true, tip_daa, finals: rows, total, ledger_root: ledger_root.to_string() })
}

// ---- op 231 `getPalwConformanceEvidence` -----------------------------------------------------------------------------------------

/// The V2 class op 231 asks about.
pub fn palw_conformance_evidence_request_v1(request: &GetPalwConformanceEvidenceRequest) -> RpcResult<Hash64> {
    parse_hash64(request.class_id.trim(), "class id")
}

/// Op 231's answer on a ConsensusV2 network.
pub fn palw_conformance_evidence_response_v1(c: &dyn ConsensusApi, class: Hash64) -> RpcResult<GetPalwConformanceEvidenceResponse> {
    use kaspa_consensus_core::palw_onboarding_v1::PalwOnboardingGateV1;
    let tip_daa = c.get_virtual_daa_score();
    let Some(read) = c.palw_conformance_evidence_v1(class) else {
        return Ok(GetPalwConformanceEvidenceResponse { available: true, class_id: class.to_string(), tip_daa, ..Default::default() });
    };
    let (gate, gate_code, gate_reason) = match read.gate {
        PalwOnboardingGateV1::NotKernelBound => ("NotKernelBound", "", ""),
        PalwOnboardingGateV1::Ready => ("Ready", "", ""),
        PalwOnboardingGateV1::Held { code, why } => ("Held", code, why),
    };
    let a = read.attempt.as_ref();
    let posted = a.and_then(|a| a.evidence);
    Ok(GetPalwConformanceEvidenceResponse {
        available: true,
        found: true,
        tip_daa,
        class_id: class.to_string(),
        lifecycle_state: a.map(|a| a.record.state.code().to_string()).unwrap_or_default(),
        last_failure: a.and_then(|a| a.record.last_failure).map(|f| f.code().to_string()).unwrap_or_default(),
        attempt_end: a.and_then(|a| a.last_end).map(|(e, _)| e.name().to_string()).unwrap_or_default(),
        attempts: a.map(|a| a.record.attempts()).unwrap_or(0),
        attempt_limit: a.map(|a| a.record.attempt_limit).unwrap_or(read.policy.retry_limit.saturating_add(1)),
        challenge_policy_id: hex(&read.policy.id()),
        challenge_policy: hex(&borsh::to_vec(&read.policy).unwrap_or_default()),
        statement_root: a.map(|a| hex(&a.commitment.statement_root())).unwrap_or_default(),
        committed_daa: a.map(|a| a.committed_daa).unwrap_or(0),
        challenge_epoch: a.map(|a| a.challenge_epoch).unwrap_or(0),
        beacon_state: read.beacon.to_string(),
        beacon_have: read.beacon_have,
        beacon_need: read.beacon_need,
        lock_position: read.lock_position.unwrap_or(0),
        beacon_output: read.beacon_output.map(|o| o.to_string()).unwrap_or_default(),
        evidence_posted: posted.is_some(),
        evidence_id: posted.map(|e| e.evidence_id.to_string()).unwrap_or_default(),
        evidence_daa: posted.map(|e| e.posted_daa).unwrap_or(0),
        window_end_daa: posted.map(|e| e.window_end_daa).unwrap_or(0),
        gate: gate.to_string(),
        gate_code: gate_code.to_string(),
        gate_reason: gate_reason.to_string(),
        attempt_row: read.attempt_row.as_deref().map(hex).unwrap_or_default(),
        evidence_row: read.evidence_row.as_deref().map(hex).unwrap_or_default(),
        program: read.program.as_deref().map(hex).unwrap_or_default(),
        ledger_root: read.ledger_root.to_string(),
        aux_root: read.aux_root.to_string(),
    })
}
