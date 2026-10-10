//! One bounded public kernel-row snapshot for independent conformance verification.
//!
//! Every page must name the same roots, header, DAA and row count. Attempt, program, Finals and
//! seal facts are then derived from those authenticated rows, rather than combined across RPC
//! calls at different tips. The served roots still need the caller's chain/state-proof trust.

use crate::onboarding_chain::{PublicCompleteCheckReadsV1, PublicConformanceReadsV1, PublicOnboardingReadsV1};
use crate::runtime_pack::commit::Refusal;
use kaspa_consensus_core::palw_kernel_route_v1::PalwKernelRouteStateV1;
use kaspa_consensus_core::palw_onboarding_v1::{
    PALW_ONBOARDING_TABLE_CONFORMANCE_ATTEMPTS_V1, PALW_ONBOARDING_TABLE_CONFORMANCE_EVIDENCE_V1,
};
use kaspa_hashes::Hash64;

/// Local retained wire-byte ceiling, not total verifier RAM, consensus capacity or a producer limit. Larger snapshots
/// need a bounded authenticated class/state proof; they must not be labelled verified here.
pub const MAX_SNAPSHOT_BYTES_V1: usize = 128 << 20;
pub type RowKeyV1 = (u8, Vec<u8>);
pub type ServedRowV1 = (u8, Vec<u8>, Vec<u8>);

#[derive(Clone, Debug)]
pub struct KernelRowsPageV1 {
    pub tip_daa: u64,
    pub ledger_root: Hash64,
    pub aux_root: Hash64,
    pub header: Vec<u8>,
    pub total_rows: u64,
    pub rows: Vec<ServedRowV1>,
    pub next: Option<RowKeyV1>,
}

#[derive(Default)]
pub struct KernelRowsSnapshotV1 {
    identity: Option<(u64, Hash64, Hash64, Vec<u8>, u64)>,
    rows: Vec<ServedRowV1>,
    bytes: usize,
    complete: bool,
}

impl KernelRowsSnapshotV1 {
    /// Transactional: a rejected page cannot replace the selected snapshot or advance its cursor.
    pub fn push(&mut self, page: KernelRowsPageV1) -> Result<Option<RowKeyV1>, Refusal> {
        let bad = |code, why: &str| Refusal::new(code, why);
        if self.complete {
            return Err(bad("SNAPSHOT_CLOSED", "a terminal page was already received"));
        }
        if page.header.len() > 64 << 10 {
            return Err(bad("SNAPSHOT_TOO_LARGE", "the route header exceeds its local reader ceiling"));
        }
        if page.total_rows > (MAX_SNAPSHOT_BYTES_V1 / 32) as u64 {
            return Err(bad("SNAPSHOT_TOO_LARGE", "the declared row count exceeds the local memory ceiling"));
        }
        let identity = (page.tip_daa, page.ledger_root, page.aux_root, page.header.clone(), page.total_rows);
        if self.identity.as_ref().is_some_and(|first| first != &identity) {
            return Err(bad("SNAPSHOT_MOVED", "the tip, roots, header or row count changed between pages; retry"));
        }
        let count = self.rows.len().checked_add(page.rows.len()).ok_or_else(|| bad("SNAPSHOT_TOO_LARGE", "row count overflow"))?;
        if count as u64 > page.total_rows {
            return Err(bad("SNAPSHOT_ROWS", "more rows than the snapshot declares"));
        }
        let mut previous = self.rows.last().map(|(table, key, _)| (*table, key.as_slice()));
        let mut bytes = if self.identity.is_none() { page.header.len() } else { self.bytes };
        for (table, key, row) in &page.rows {
            let current = (*table, key.as_slice());
            if previous.is_some_and(|prev| prev >= current) {
                return Err(bad("SNAPSHOT_ORDER", "rows must advance strictly; a duplicate or repeated cursor cannot be accepted"));
            }
            previous = Some(current);
            bytes = bytes
                .checked_add(key.len())
                .and_then(|n| n.checked_add(row.len()))
                .and_then(|n| n.checked_add(32))
                .ok_or_else(|| bad("SNAPSHOT_TOO_LARGE", "byte count overflow"))?;
        }
        if bytes > MAX_SNAPSHOT_BYTES_V1 {
            return Err(bad("SNAPSHOT_TOO_LARGE", "the snapshot exceeds the local memory ceiling"));
        }
        if let Some(next) = &page.next {
            if previous != Some((next.0, next.1.as_slice())) || page.rows.is_empty() || count as u64 >= page.total_rows {
                return Err(bad("SNAPSHOT_CURSOR", "a nonterminal cursor must name this page's last row and leave rows to read"));
            }
        } else if count as u64 != page.total_rows {
            return Err(bad("SNAPSHOT_INCOMPLETE", "the terminal page omitted declared rows"));
        }
        let next = page.next.clone();
        self.identity.get_or_insert(identity);
        self.rows.extend(page.rows);
        self.bytes = bytes;
        self.complete = next.is_none();
        Ok(next)
    }

    pub fn into_reads(self, class: Hash64) -> Result<PublicConformanceReadsV1, Refusal> {
        match self.into_onboarding_reads_v1(class)? {
            PublicOnboardingReadsV1::Sampled(reads) => Ok(reads),
            PublicOnboardingReadsV1::CompleteCheck(_) => {
                Err(Refusal::new("COMPLETE_CHECK", "use into_onboarding_reads_v1 and the complete-check verifier for this attempt"))
            }
        }
    }

    /// Dispatch by the committed policy after checking the same program/attempt bindings.
    pub fn into_onboarding_reads_v1(self, class: Hash64) -> Result<PublicOnboardingReadsV1, Refusal> {
        if !self.complete {
            return Err(Refusal::new("SNAPSHOT_INCOMPLETE", "no complete snapshot was received"));
        }
        let (tip_daa, ledger_root, aux_root, header, _) = self.identity.expect("a completed snapshot has a header");
        let route = PalwKernelRouteStateV1::from_served_rows_v1(&header, self.rows, &ledger_root, &aux_root)
            .map_err(|e| Refusal::new("ROWS_NOT_THE_CHAIN", e))?;
        if route.header.scalars.daa > tip_daa {
            return Err(Refusal::new("SNAPSHOT_CLOCK", "the rows are newer than the served tip"));
        }
        let key = borsh::to_vec(&class).expect("digest serializes");
        let attempt_row = route
            .aux
            .get(&(PALW_ONBOARDING_TABLE_CONFORMANCE_ATTEMPTS_V1, key.clone()))
            .ok_or_else(|| Refusal::new("NO_ATTEMPT", "the requested class has no attempt in this snapshot"))?
            .clone();
        let attempt =
            route.conformance_attempt_v1(&class).ok_or_else(|| Refusal::new("ROW_MALFORMED", "the attempt row does not decode"))?;
        let binding = route
            .kernel_binding_v1(&class)
            .ok_or_else(|| Refusal::new("NO_KERNEL_BINDING", "the requested class has no kernel binding in this snapshot"))?;
        let ledger = route.ledger().map_err(|e| Refusal::new("ROWS_MALFORMED", e))?;
        let kernel = ledger
            .classes
            .get(&binding.kernel_class.as_bytes())
            .or_else(|| ledger.conformance_classes.get(&binding.kernel_class.as_bytes()).map(|(_, row)| row))
            .ok_or_else(|| Refusal::new("NO_KERNEL_CLASS", "the bound program is absent from this snapshot"))?;
        if attempt.commitment.candidate_id != class.as_bytes()
            || attempt.commitment.program_root != kernel.plan.program_root
            || attempt.commitment.verification_plan_root != kernel.plan.root()
            || binding.plan_root.as_bytes() != kernel.plan.root()
            || binding.kernel_param_root.as_bytes() != kernel.param_commitments.root()
            || attempt.commitment.kernel_descriptor_id != kernel.descriptor.digest()
            || attempt.commitment.challenge_policy_id != binding.challenge_policy_id.as_bytes()
        {
            return Err(Refusal::new("SNAPSHOT_BINDING", "the attempt, kernel link and public program are not the same statement"));
        }
        if attempt.is_complete_check() {
            kaspa_consensus_core::palw_opv_bootstrap_v1::palw_complete_check_domain_v1(&kernel.program, kernel.plan.max_positions)
                .map_err(|e| Refusal::new("NOT_COMPLETELY_CHECKABLE", e))?;
            return Ok(PublicOnboardingReadsV1::CompleteCheck(PublicCompleteCheckReadsV1 {
                attempt_row,
                program: kernel.program_bytes.clone(),
                max_positions: kernel.plan.max_positions,
                kernel_param_root: binding.kernel_param_root,
            }));
        }
        Ok(PublicOnboardingReadsV1::Sampled(PublicConformanceReadsV1 {
            attempt_row,
            evidence_row: route.aux.get(&(PALW_ONBOARDING_TABLE_CONFORMANCE_EVIDENCE_V1, key)).cloned(),
            events: route.beacon_events_v1().map_err(|e| Refusal::new("ROWS_MALFORMED", e))?,
            sealed_sources: if attempt.is_sealed_source() {
                route.beacon_sealed_sources_v1().map_err(|e| Refusal::new("ROWS_MALFORMED", e))?
            } else {
                Vec::new()
            },
            tip_daa,
            program: kernel.program_bytes.clone(),
        }))
    }
}
