//! Constant-retention receipt transcript audit. Every independently verified
//! state is consumed in order; only window endpoints and the previous state
//! remain in memory. Neither a shortened transcript nor sampled states suffice.
use super::*;
use std::sync::Arc;

pub const RECEIPT_STREAM_METADATA_BYTES_V1: usize = 128 << 20;
/// Explicit local audit profile, matching the long-window collector. This is
/// neither a consensus quota nor permission to sample a larger transcript.
pub const RECEIPT_STREAM_MAX_POINTS_V1: usize = 10_000_000;

#[derive(Default)]
struct Endpoints {
    baseline: Option<(usize, Arc<AnchoredReceiptPointV1>)>,
    end: Option<(usize, Arc<AnchoredReceiptPointV1>)>,
    completed: Option<Arc<AnchoredReceiptPointV1>>,
}

/// Private checked points are the sole input; reports/counters cannot construct
/// a point. An error poisons the accumulator so ignoring a rejected transition
/// cannot turn a discontinuous stream into a successful audit.
pub struct ReceiptWindowAccumulatorV1 {
    contract: ReceiptWindowContractV1,
    prior: Option<Arc<AnchoredReceiptPointV1>>,
    endpoints: Vec<Endpoints>,
    funded: BTreeSet<[u8; 64]>,
    issued: BTreeSet<[u8; 64]>,
    finalized: BTreeSet<[u8; 64]>,
    retired: BTreeSet<[u8; 64]>,
    checkpoints: BTreeMap<usize, serde_json::Value>,
    first: Option<serde_json::Value>,
    last: Option<serde_json::Value>,
    transcript: [u8; 64],
    points: usize,
    failed: bool,
}

impl ReceiptWindowAccumulatorV1 {
    pub fn new(contract: ReceiptWindowContractV1) -> Result<Self, String> {
        contract.check()?;
        let count = contract.windows.len();
        Ok(Self {
            contract,
            prior: None,
            endpoints: (0..count).map(|_| Endpoints::default()).collect(),
            funded: BTreeSet::new(),
            issued: BTreeSet::new(),
            finalized: BTreeSet::new(),
            retired: BTreeSet::new(),
            checkpoints: BTreeMap::new(),
            first: None,
            last: None,
            transcript: [0; 64],
            points: 0,
            failed: false,
        })
    }

    pub fn push(&mut self, point: AnchoredReceiptPointV1) -> Result<(), String> {
        if self.failed {
            return Err("receipt stream previously refused a transition".into());
        }
        self.failed = true;
        self.push_inner(point)?;
        self.failed = false;
        Ok(())
    }

    fn push_inner(&mut self, point: AnchoredReceiptPointV1) -> Result<(), String> {
        if self.points >= RECEIPT_STREAM_MAX_POINTS_V1 {
            return Err("receipt stream explicit state reservation exhausted".into());
        }
        let s = &point.snapshot;
        let queue: BTreeSet<_> = s.queue().jobs.iter().copied().collect();
        if let Some(previous) = &self.prior {
            if point.parent != previous.child || s.daa() < previous.snapshot.daa() {
                return Err("receipt stream selected-parent state skipped/reordered/reorged".into());
            }
            for (id, old) in previous.snapshot.jobs() {
                let new = s.jobs().get(id).ok_or("receipt stream original collected before cumulative audit completed")?;
                let mut a = old.clone();
                let mut b = new.clone();
                a.claim = None;
                b.claim = None;
                a.committed = None;
                b.committed = None;
                a.challenge_end = None;
                b.challenge_end = None;
                a.terminal.clear();
                b.terminal.clear();
                if a != b
                    || old.claim.is_some() && old.claim != new.claim
                    || old.committed.is_some() && old.committed != new.committed
                    || matches!(old.terminal.as_str(), "Final" | "Convicted" | "Refunded") && old.terminal != new.terminal
                {
                    return Err("receipt stream original source/scope/nonce/claim or terminal changed".into());
                }
            }
            for id in &previous.snapshot.queue().jobs {
                if !queue.contains(id) {
                    let j = s.jobs().get(id).ok_or("receipt stream queue drop lacks original terminal")?;
                    if !matches!(j.terminal.as_str(), "Final" | "Convicted" | "Refunded") {
                        return Err("receipt stream queue drop before genuine terminal".into());
                    }
                    self.retired.insert(*id);
                }
            }
        }
        if queue.iter().any(|id| self.retired.contains(id)) {
            return Err("receipt stream retired original reentered queue".into());
        }
        for (id, j) in s.jobs() {
            self.funded.insert(*id);
            if j.claim.is_some() {
                if j.committed.is_none_or(|d| d < j.admitted || d >= j.commit_deadline) || j.challenge_end.is_none() {
                    return Err("receipt stream claim lacks original accepted clock/opportunity".into());
                }
                self.issued.insert(*id);
            }
            if j.terminal == "Final" {
                let f = s.finals().get(id).ok_or("receipt stream genuine independent Final32 absent")?;
                if Some(f.claim) != j.claim || f.established_daa > s.daa() || f.established_daa < j.challenge_end.unwrap_or(u64::MAX) {
                    return Err("receipt stream Final clock/claim mismatch".into());
                }
                self.finalized.insert(*id);
            }
        }
        if self.funded.len() > kaspa_consensus_core::palw_native_receipt_windows_v1::MAX_RECEIPT_WINDOW_JOBS_V1 {
            return Err("receipt stream original tracked job cap".into());
        }
        // Full checked Master records, not a claimed model flag, enter the
        // transcript root alongside the single-component state and queue roots.
        self.transcript = object_id(
            b"misaka-palw/audit/consecutive-receipt-transcript/v2",
            &(self.transcript, self.points as u64, point.child, point.parent, s.root(), &point.masters),
        );
        let summary = serde_json::json!({"point":self.points,"child":point.child.to_string(),
            "selected_parent":point.parent.to_string(),"daa":s.daa(),
            "snapshot_root":crate::runtime_pack::commit::hex(&s.root()),
            "queue_root":crate::runtime_pack::commit::hex(&s.queue().root),
            "physical_root":crate::runtime_pack::commit::hex(&s.queue().physical_root),
            "live":s.queue().live,"packets":s.queue().packets,"owned_bytes":s.queue().owned_bytes,
            "funded_cumulative":self.funded.len(),"issued_cumulative":self.issued.len(),
            "finalized_cumulative":self.finalized.len(),"retired_cumulative":self.retired.len(),
            "transcript_root":crate::runtime_pack::commit::hex(&self.transcript)});
        if self.first.is_none() {
            self.first = Some(summary.clone());
        }
        self.last = Some(summary.clone());
        let point = Arc::new(point);
        for (window, endpoints) in self.contract.windows.iter().zip(self.endpoints.iter_mut()) {
            if point.snapshot.daa() < window.start_daa {
                endpoints.baseline = Some((self.points, point.clone()));
            }
            if point.snapshot.daa() < window.end_daa {
                endpoints.end = Some((self.points, point.clone()));
            }
            let deadline = window.end_daa.checked_add(window.maximum_final_lag_daa).ok_or("receipt stream Final tail overflow")?;
            if endpoints.completed.is_none() && point.snapshot.daa() >= deadline {
                endpoints.completed = Some(point.clone());
                self.checkpoints.insert(self.points, summary.clone());
            }
        }
        self.points = self.points.checked_add(1).ok_or("receipt stream point count overflow")?;
        self.prior = Some(point);
        // At most the previous point and three endpoints per frozen window.
        // Arc duplicates are conservatively counted twice; raw proof bytes have
        // already been dropped by the caller before its next iteration.
        let mut bytes = self.prior.as_ref().map_or(0, |p| p.metadata_bytes());
        for e in &self.endpoints {
            for point in [e.baseline.as_ref().map(|(_, p)| p), e.end.as_ref().map(|(_, p)| p), e.completed.as_ref()] {
                bytes = bytes.checked_add(point.map_or(0, |p| p.metadata_bytes())).ok_or("receipt retained metadata overflow")?;
            }
        }
        bytes = bytes.checked_add(self.funded.len().saturating_mul(256)).ok_or("receipt stream set metadata overflow")?;
        if bytes > RECEIPT_STREAM_METADATA_BYTES_V1 {
            return Err("receipt stream retained endpoints exceed explicit metadata profile".into());
        }
        Ok(())
    }

    pub fn finish(self) -> Result<serde_json::Value, String> {
        if self.failed || self.points < 3 {
            return Err("receipt stream incomplete or previously refused".into());
        }
        let mut windows = Vec::with_capacity(self.endpoints.len());
        for (window, endpoints) in self.contract.windows.iter().zip(&self.endpoints) {
            let (start, first) = endpoints.baseline.as_ref().ok_or("receipt stream beginning unobserved")?;
            let (end, last) = endpoints.end.as_ref().ok_or("receipt stream ending unobserved")?;
            let completed = endpoints.completed.as_ref().ok_or("receipt stream bounded Final tail unobserved")?;
            if start >= end {
                return Err("receipt stream no sustained original chain opportunity".into());
            }
            windows.push(audit_receipt_window_v1(&self.contract, window, first, last, completed, *start, *end)?);
        }
        let mut checkpoints = self.checkpoints;
        checkpoints.insert(0, self.first.ok_or("receipt stream first summary absent")?);
        checkpoints.insert(self.points - 1, self.last.ok_or("receipt stream final summary absent")?);
        Ok(serde_json::json!({"schema":"misaka.rfc02.proof-derived-receipt-windows.v2",
            "contract_root":self.contract.root()?,"cohort_sha256":self.contract.cohort_sha256,
            "original_model_roots":self.contract.original_model_roots,"receipt_window_daa":self.contract.receipt_window_daa,
            "minimum_independent_operators":self.contract.minimum_independent_operators,"windows":windows,
            "cumulative_accounting":checkpoints.into_values().collect::<Vec<_>>(),
            "transcript_points":self.points,"consecutive_transcript_root":crate::runtime_pack::commit::hex(&self.transcript),
            "retention_profile":"previous-and-frozen-endpoints;every-point-verified;no-sampling",
            "qualified_native_profile":true,"intended_claims_and_queue_accounting_verified":true}))
    }
}
