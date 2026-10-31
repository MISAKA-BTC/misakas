//! Owning39 sustained issuance observations. Frozen JSON describes intent;
//! only private Core views of the original native rows describe acceptance.
//! Hashes and Merkle openings authenticate data identity, not LLM correctness.
//! Reexecution, public local counterexamples, deterministic courts and the
//! explicit sampled-validator/honest-fraction premise remain independent.
use borsh::BorshDeserialize;
use kaspa_consensus_core::{
    Hash64,
    config::params::Params,
    palw_flat_execution_capacity_v22::{NativeCarriageGlobalV22, NativeCarriageLeaseV22},
    palw_native_audit_proof_v39::{NativeAuditRowReaderV39, VerifiedNativeAuxRowsV39},
    palw_native_master_v39::{NativeMasterComponentV39, NativeMasterRecordV39},
    palw_native_paged_carriage_v29::NativePagedCarriageRecordV29,
    palw_native_queue_v30::{NATIVE_QUEUE_ROW_LIMIT_V30, NativeQueueGlobalV30, NativeQueueLeaseV30},
};
use misaka_palw_kernel::hash::{Digest, object_id};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const RECEIPT_MASTERS_MAX_V39: usize = 512;
pub const RECEIPT_COMPONENTS_MAX_V39: usize = 4096;

/// A downstream native job id cannot be frozen before its upstream Final's
/// actual tensors and EOS exist. Freeze its original Master stage instead;
/// learn an id ONLY from the checked original Fund84, then never replace it.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntendedComponentV39 {
    pub stage: u16,
    pub class: Hash64,
    pub source: Hash64,
    pub profile: Hash64,
    pub program: Hash64,
    pub inventory: Hash64,
    pub real_parameter: Hash64,
    pub input_declarations: Hash64,
    pub output_declaration: Hash64,
    pub original_context: u32,
    pub maximum_positions: u32,
    pub maximum_scope: Hash64,
    pub maximum_resources: Hash64,
    pub requester_operator: Hash64,
    pub producer_operator: Hash64,
    pub job_if_already_known: Option<Hash64>,
}
impl IntendedComponentV39 {
    fn matches(&self, c: &NativeMasterComponentV39) -> bool {
        self.stage == c.stage
            && self.original_context == c.original_context
            && self.maximum_positions == c.maximum_positions
            && [
                (self.class, c.class),
                (self.source, c.source),
                (self.profile, c.profile),
                (self.program, c.program),
                (self.inventory, c.inventory),
                (self.real_parameter, c.parameter),
                (self.input_declarations, c.input),
                (self.output_declaration, c.output),
                (self.maximum_scope, c.maximum_scope),
                (self.maximum_resources, c.maximum_resources),
            ]
            .iter()
            .all(|(a, b)| a.as_bytes() == *b)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntendedMasterV39 {
    pub member: String,
    pub master: Hash64,
    pub sequence: u64,
    pub requester_operator: Hash64,
    pub complete_source: Hash64,
    pub manifest: Hash64,
    pub pipeline: Hash64,
    pub request: Hash64,
    pub class_set: Hash64,
    pub profile_set: Hash64,
    pub program_set: Hash64,
    pub inventory_set: Hash64,
    pub real_parameter_set: Hash64,
    pub maximum_scope_set: Hash64,
    pub maximum_resource_set: Hash64,
    pub components: Vec<IntendedComponentV39>,
    pub issuance_not_before: u64,
    pub issuance_deadline: u64,
}
impl IntendedMasterV39 {
    fn matches(&self, m: &NativeMasterRecordV39) -> bool {
        m.version == 39
            && m.id == self.master.as_bytes()
            && m.sequence == self.sequence
            && self.components.len() == m.components.len()
            && [
                (self.requester_operator, m.operator),
                (self.complete_source, m.complete_source),
                (self.manifest, m.manifest),
                (self.pipeline, m.pipeline),
                (self.request, m.request),
                (self.class_set, m.class_set),
                (self.profile_set, m.profile_set),
                (self.program_set, m.program_set),
                (self.inventory_set, m.inventory_set),
                (self.real_parameter_set, m.parameter_set),
                (self.maximum_scope_set, m.maximum_scope_set),
                (self.maximum_resource_set, m.maximum_resource_set),
            ]
            .iter()
            .all(|(a, b)| a.as_bytes() == *b)
            && self.components.iter().zip(&m.components).all(|(a, b)| a.matches(b))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReceiptWindowV39 {
    pub start_daa: u64,
    pub end_daa: u64,
    pub issuance_bin_daa: u64,
    pub minimum_commits_per_bin: usize,
    pub maximum_final_lag_daa: u64,
    pub intended: Vec<IntendedMasterV39>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReceiptWindowContractV39 {
    pub schema: String,
    pub cohort_sha256: String,
    pub receipt_window_daa: u64,
    pub minimum_independent_operators: usize,
    /// Original aggregate R over ALL original component inventories. A single
    /// component R, conversion marker or source-fidelity boolean is insufficient.
    pub original_model_inventory_sets: BTreeMap<String, Hash64>,
    pub original_native_policy: Hash64,
    pub original_final_policy: Hash64,
    pub original_queue_policy: Hash64,
    pub original_physical_service: Hash64,
    pub windows: Vec<ReceiptWindowV39>,
}
fn nonzero(h: Hash64) -> bool {
    h.as_bytes() != [0; 64]
}
impl ReceiptWindowContractV39 {
    pub fn check(&self) -> Result<(), String> {
        if self.schema != "misaka.rfc02.native-receipt-window-contract.v39"
            || self.cohort_sha256.len() != 64
            || !self.cohort_sha256.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || self.receipt_window_daa == 0
            || self.windows.len() != 2
            || !(2..=128).contains(&self.minimum_independent_operators)
            || self.original_model_inventory_sets.is_empty()
            || self.original_model_inventory_sets.len() > RECEIPT_MASTERS_MAX_V39
            || self.original_model_inventory_sets.iter().any(|(name, r)| name.is_empty() || name.len() > 256 || !nonzero(*r))
            || [self.original_native_policy, self.original_final_policy, self.original_queue_policy, self.original_physical_service]
                .iter()
                .any(|h| !nonzero(*h))
        {
            return Err("receipt39 exact original contract/schema/policy/denominator required".into());
        }
        let (mut masters, mut jobs, mut components, mut prior) = (BTreeSet::new(), BTreeSet::new(), 0usize, None);
        for w in &self.windows {
            let span = w.end_daa.checked_sub(w.start_daa).ok_or("receipt39 window clock overflow")?;
            if span < self.receipt_window_daa
                || prior.is_some_and(|end| w.start_daa < end)
                || w.issuance_bin_daa == 0
                || w.issuance_bin_daa > span / 2
                || span % w.issuance_bin_daa != 0
                || span / w.issuance_bin_daa > 1024
                || w.minimum_commits_per_bin == 0
                || w.maximum_final_lag_daa == 0
                || w.maximum_final_lag_daa > w.issuance_bin_daa
                || w.end_daa.checked_add(w.maximum_final_lag_daa).is_none()
                || w.intended.is_empty()
            {
                return Err("receipt39 two sustained nonoverlapping windows/bins/Final tail required".into());
            }
            prior = Some(w.end_daa);
            let mut members = BTreeSet::new();
            for m in &w.intended {
                if !masters.insert(m.master)
                    || masters.len() > RECEIPT_MASTERS_MAX_V39
                    || self.original_model_inventory_sets.get(&m.member) != Some(&m.inventory_set)
                    || m.issuance_not_before < w.start_daa
                    || m.issuance_deadline > w.end_daa
                    || m.issuance_deadline <= m.issuance_not_before
                    || m.components.is_empty()
                    || m.components.len() > 64
                    || [
                        m.master,
                        m.requester_operator,
                        m.complete_source,
                        m.manifest,
                        m.pipeline,
                        m.request,
                        m.class_set,
                        m.profile_set,
                        m.program_set,
                        m.inventory_set,
                        m.real_parameter_set,
                        m.maximum_scope_set,
                        m.maximum_resource_set,
                    ]
                    .iter()
                    .any(|h| !nonzero(*h))
                {
                    return Err("receipt39 original Master/source/sets/cadence differs or repeats".into());
                }
                members.insert(m.member.clone());
                components = components
                    .checked_add(m.components.len())
                    .filter(|n| *n <= RECEIPT_COMPONENTS_MAX_V39)
                    .ok_or("receipt39 original component denominator cap")?;
                for (i, c) in m.components.iter().enumerate() {
                    if c.stage as usize != i
                        || c.requester_operator != m.requester_operator
                        || c.requester_operator == c.producer_operator
                        || c.maximum_positions == 0
                        || c.maximum_positions > c.original_context
                        || [
                            c.class,
                            c.source,
                            c.profile,
                            c.program,
                            c.inventory,
                            c.real_parameter,
                            c.input_declarations,
                            c.output_declaration,
                            c.maximum_scope,
                            c.maximum_resources,
                            c.requester_operator,
                            c.producer_operator,
                        ]
                        .iter()
                        .any(|h| !nonzero(*h))
                        || c.job_if_already_known.is_some_and(|id| !nonzero(id) || !jobs.insert(id))
                    {
                        return Err("receipt39 ordered original stage/operator/max/optional job differs".into());
                    }
                }
            }
            if members.len() != self.original_model_inventory_sets.len() {
                return Err("receipt39 each original model must remain in both window denominators".into());
            }
            for bin in 0..span / w.issuance_bin_daa {
                let from = w.start_daa + bin * w.issuance_bin_daa;
                let until = from + w.issuance_bin_daa;
                if w.intended.iter().filter(|m| m.issuance_not_before >= from && m.issuance_deadline <= until).count()
                    < w.minimum_commits_per_bin
                {
                    return Err("receipt39 intended sustained cadence lacks a bin".into());
                }
            }
        }
        Ok(())
    }
    pub fn root(&self) -> Result<Digest, String> {
        self.check()?;
        Ok(object_id(b"misaka-palw/audit/frozen-Master-receipt-intent/v39", &serde_json::to_vec(self).map_err(|e| e.to_string())?))
    }
}

/// Readonly complete shared queue projection. Decoded rows are accepted only
/// after configured Queue30 policy + SAME shared physical22 totals agree.
#[derive(Clone)]
#[allow(dead_code)]
struct CheckedQueueV39 {
    global: NativeQueueGlobalV30,
    physical: NativeCarriageGlobalV22,
    entries: BTreeMap<Digest, QueueOriginalV39>,
}
#[derive(Clone)]
#[allow(dead_code)]
struct QueueOriginalV39 {
    funding: Digest,
    queue: NativeQueueLeaseV30,
    physical: NativeCarriageLeaseV22,
    carriage: NativePagedCarriageRecordV29,
}
fn row<R: NativeAuditRowReaderV39, T: BorshDeserialize>(
    rows: &VerifiedNativeAuxRowsV39<R>,
    key: Vec<u8>,
    cap: usize,
) -> Result<T, String> {
    let raw = rows.row(92, &key, cap)?.ok_or("receipt39 required original shared row absent")?;
    borsh::from_slice(&raw).map_err(|_| "receipt39 original shared row exact grammar".into())
}
fn queue<R: NativeAuditRowReaderV39>(rows: &VerifiedNativeAuxRowsV39<R>, p: &Params, daa: u64) -> Result<CheckedQueueV39, String> {
    let f = p.palw_native_final_v27.as_ref().filter(|f| f.activation.is_active(daa)).ok_or("receipt39 Final policy inactive")?;
    let policy = &f.policy.queue_policy;
    let native = &p
        .palw_native_execution_v39
        .as_ref()
        .filter(|f| f.activation.is_active(daa))
        .ok_or("receipt39 own execution policy inactive")?
        .policies;
    native.check(native.network_domain)?;
    if native.final_policy != f.policy.root() {
        return Err("receipt39 own execution/Final roots differ".into());
    }
    let (gr, g): (Digest, NativeQueueGlobalV30) = row(rows, borsh::to_vec(&(30u8, 0u8)).unwrap(), NATIVE_QUEUE_ROW_LIMIT_V30)?;
    let (pr, physical): (Digest, NativeCarriageGlobalV22) = row(rows, borsh::to_vec(&(22u8, 0u8)).unwrap(), 512)?;
    if gr != g.root() || pr != physical.root() || g.service != native.physical_service {
        return Err("receipt39 original shared roots/service differ".into());
    }
    let mut entries = BTreeMap::new();
    let mut density = 0u64;
    let mut minimum = u64::MAX;
    for (id, funding, lease_root, packets, owned) in g.audit_entries_v1(policy, &physical)? {
        let (lr, l): (Digest, NativeQueueLeaseV30) = row(rows, borsh::to_vec(&(30u8, *id, 1u8)).unwrap(), 1024)?;
        let (plr, pl): (Digest, NativeCarriageLeaseV22) = row(rows, borsh::to_vec(&(22u8, *id)).unwrap(), 1024)?;
        // No old29/32 record is promoted into an original39 physical promise.
        let (cr, c): (Digest, NativePagedCarriageRecordV29) = row(rows, borsh::to_vec(&(39u8, *id, 4u8)).unwrap(), 4096)?;
        if lr != *lease_root
            || lr != l.root()
            || l.version != 30
            || l.retired
            || l.job != *id
            || l.original_funding != *funding
            || l.original_policy != policy.root()
            || l.packets != packets
            || l.owned_bytes != owned
            || plr != pl.root()
            || pl.version != 22
            || pl.retired
            || pl.original_job != *id
            || l.original_physical_lease != plr
            || l.original_consent != pl.original_consent
            || c.version != 39
            || cr != c.root()
            || c.service != g.service
            || pl.plan != cr
            || pl.density != c.density
            || pl.minimum_deadline != c.minimum_deadline
        {
            return Err("receipt39 exact original queue/physical/carriage lineage differs or unsupported mixed namespace".into());
        }
        density = density.checked_add(pl.density).ok_or("receipt39 physical density overflow")?;
        minimum = minimum.min(pl.minimum_deadline);
        entries.insert(*id, QueueOriginalV39 { funding: *funding, queue: l, physical: pl, carriage: c });
    }
    if density != physical.density
        || physical.live as usize != entries.len()
        || (physical.live > 0 && physical.minimum_deadline > minimum)
        || (physical.live == 0 && (physical.density != 0 || physical.minimum_deadline != u64::MAX))
    {
        return Err("receipt39 incomplete original shared physical accounting".into());
    }
    Ok(CheckedQueueV39 { global: g, physical, entries })
}

// ---- Receipt point, cumulative accumulator and two-window conclusion (A6/A8) ----

/// One component's observation, re-derived by the Core job audit factory.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ComponentObservationV39 {
    pub stage: u16,
    pub job: Hash64,
    pub funding: Hash64,
    pub admitted: u64,
    pub producer_operator: Hash64,
    pub terminal: String,
    pub claim: Option<Hash64>,
    pub committed: Option<u64>,
    pub final_after: Option<u64>,
    pub single_credit: Option<Hash64>,
    pub established: Option<u64>,
}
/// One intended Master at one anchored state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MasterObservationV39 {
    pub master: Hash64,
    pub record: Hash64,
    pub components: Vec<Option<ComponentObservationV39>>,
    /// `(aggregate root, established, single-credit set)` of the genuine
    /// aggregate Final, re-derived from every component audit at this state.
    pub aggregate: Option<(Hash64, u64, Hash64)>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct QueueObservationV39 {
    pub live: u32,
    pub packets: u32,
    pub owned_bytes: u64,
    pub jobs: BTreeSet<Hash64>,
}
/// Private checked projection of ONE anchored state. Only
/// `AnchoredReceiptPointV39::derive` constructs it, from the authenticated
/// stream and Core factories; JSON never becomes a point.
#[derive(Clone)]
#[allow(dead_code)]
pub struct AnchoredReceiptPointV39 {
    child: Hash64,
    parent: Hash64,
    daa: u64,
    state: Hash64,
    auxiliary: Hash64,
    queue: QueueObservationV39,
    masters: BTreeMap<Hash64, MasterObservationV39>,
}
fn h64(d: &Digest) -> Hash64 {
    Hash64::from_bytes(*d)
}
impl AnchoredReceiptPointV39 {
    pub fn daa(&self) -> u64 {
        self.daa
    }
    pub fn masters(&self) -> &BTreeMap<Hash64, MasterObservationV39> {
        &self.masters
    }
    pub fn derive(
        stream: &crate::native_audit_io_v39::AnchoredNativeAuxStreamV39,
        params: &Params,
        contract: &ReceiptWindowContractV39,
        archive: Option<&dyn kaspa_consensus_core::palw_native_execution_v39::NativeOriginalSourceArchiveV39>,
    ) -> Result<Self, String> {
        use kaspa_consensus_core::palw_state_v2::PalwChainStateV2 as S;
        contract.check()?;
        let rows = stream.auxiliary();
        let anchor = stream.anchor();
        let daa = anchor.daa();
        let q = queue(rows, params, daa)?;
        let queue = QueueObservationV39 {
            live: q.global.live,
            packets: q.global.packets,
            owned_bytes: q.global.owned_bytes,
            jobs: q.entries.keys().map(h64).collect(),
        };
        let mut masters = BTreeMap::new();
        for intended in contract.windows.iter().flat_map(|w| w.intended.iter()) {
            let id = intended.master.as_bytes();
            let key = borsh::to_vec(&(39u8, 80u8, id)).map_err(|e| e.to_string())?;
            if !rows.contains(112, &key) {
                continue; // not yet registered at this state; the window decides
            }
            let source = S::native_master_sources_from_stream_with_archive_v39(rows, anchor, params, id, archive)?;
            if !intended.matches(source.record()) {
                return Err("receipt39 registered Master differs from its frozen original intent".into());
            }
            let mut components = Vec::with_capacity(source.funded().len());
            let mut audits = Vec::new();
            for funded in source.funded() {
                let Some(funded) = funded else {
                    components.push(None);
                    continue;
                };
                let audit = S::native_job_audit_from_stream_v39(rows, anchor, params, funded.job, archive)?;
                let claim = audit.claim();
                components.push(Some(ComponentObservationV39 {
                    stage: funded.stage,
                    job: h64(&funded.job),
                    funding: h64(&funded.funding),
                    admitted: funded.admitted,
                    producer_operator: h64(&funded.producer_operator),
                    terminal: format!("{:?}", audit.terminal()),
                    claim: claim.map(|c| h64(&c.claim)),
                    committed: claim.map(|c| c.committed),
                    final_after: claim.map(|c| c.final_after),
                    single_credit: audit.credit().map(|c| h64(&c.root())),
                    established: audit.credit().map(|c| c.established),
                }));
                audits.push(audit);
            }
            let record = h64(&source.record().root());
            let aggregate = if audits.len() == components.len() {
                kaspa_consensus_core::palw_native_master_v39::derive_native_master_final_audit_v39(
                    rows,
                    anchor,
                    params,
                    std::sync::Arc::new(source),
                    &audits,
                )?
                .map(|a| (h64(&a.record().root()), a.record().established, h64(&a.record().single_credit_set)))
            } else {
                None
            };
            masters.insert(intended.master, MasterObservationV39 { master: intended.master, record, components, aggregate });
        }
        Ok(Self { child: anchor.child(), parent: anchor.parent(), daa, state: anchor.inner(), auxiliary: rows.root(), queue, masters })
    }
}
/// Bounded transcript accumulator. Every consecutive selected-parent state is
/// required; a component's job id is fixed at its FIRST authentic Fund84
/// observation (or the frozen `job_if_already_known`) and never replaced; a
/// terminal never reverts; a queue drop requires the job's genuine terminal.
pub struct ReceiptAccumulatorV39<'a> {
    contract: &'a ReceiptWindowContractV39,
    prior: Option<AnchoredReceiptPointV39>,
    fixed: BTreeMap<(Hash64, u16), Hash64>,
    terminal: BTreeMap<Hash64, String>,
    retired: BTreeSet<Hash64>,
    points: Vec<(u64, Hash64, Hash64, QueueObservationV39)>,
    masters_final: BTreeMap<Hash64, MasterObservationV39>,
    first_observed: BTreeMap<Hash64, u64>,
}
impl<'a> ReceiptAccumulatorV39<'a> {
    pub fn new(contract: &'a ReceiptWindowContractV39) -> Result<Self, String> {
        contract.check()?;
        let mut fixed = BTreeMap::new();
        for m in contract.windows.iter().flat_map(|w| w.intended.iter()) {
            for c in &m.components {
                if let Some(job) = c.job_if_already_known {
                    fixed.insert((m.master, c.stage), job);
                }
            }
        }
        Ok(Self {
            contract,
            prior: None,
            fixed,
            terminal: BTreeMap::new(),
            retired: BTreeSet::new(),
            points: Vec::new(),
            masters_final: BTreeMap::new(),
            first_observed: BTreeMap::new(),
        })
    }
    pub fn push(&mut self, point: AnchoredReceiptPointV39) -> Result<(), String> {
        if self.points.len() >= 1 << 22 {
            return Err("receipt39 transcript point cap".into());
        }
        if let Some(prior) = &self.prior {
            if point.parent != prior.child || point.daa < prior.daa {
                return Err("receipt39 selected-parent state skipped/reordered/reorged".into());
            }
            for (id, old) in &prior.masters {
                let new = point.masters.get(id).ok_or("receipt39 registered original Master disappeared")?;
                if new.record != old.record || new.components.len() != old.components.len() {
                    return Err("receipt39 original Master record changed".into());
                }
                if old.aggregate.is_some() && new.aggregate != old.aggregate {
                    return Err("receipt39 original aggregate Final changed".into());
                }
                for (a, b) in old.components.iter().zip(&new.components) {
                    if let Some(a) = a {
                        let b = b.as_ref().ok_or("receipt39 funded original component disappeared")?;
                        if a.job != b.job
                            || a.funding != b.funding
                            || a.admitted != b.admitted
                            || a.claim.is_some() && a.claim != b.claim
                        {
                            return Err("receipt39 original component Fund/claim changed".into());
                        }
                    }
                }
            }
            for job in prior.queue.jobs.difference(&point.queue.jobs) {
                match self.terminal.get(job).map(String::as_str) {
                    Some("Final" | "Refunded" | "Convicted") => {
                        self.retired.insert(*job);
                    }
                    _ => return Err("receipt39 queue drop before a genuine original terminal".into()),
                }
            }
        }
        for job in &point.queue.jobs {
            if self.retired.contains(job) {
                return Err("receipt39 retired original re-entered the queue".into());
            }
        }
        for (id, m) in &point.masters {
            self.first_observed.entry(*id).or_insert(point.daa);
            for c in m.components.iter().flatten() {
                match self.fixed.get(&(*id, c.stage)) {
                    Some(job) if *job != c.job => return Err("receipt39 component job differs from its first authentic Fund84".into()),
                    Some(_) => {}
                    None => {
                        self.fixed.insert((*id, c.stage), c.job);
                    }
                }
                if let Some(old) = self.terminal.get(&c.job)
                    && matches!(old.as_str(), "Final" | "Refunded" | "Convicted")
                    && *old != c.terminal
                {
                    return Err("receipt39 original terminal reverted".into());
                }
                self.terminal.insert(c.job, c.terminal.clone());
            }
            self.masters_final.insert(*id, m.clone());
        }
        self.points.push((point.daa, point.child, point.parent, point.queue.clone()));
        self.prior = Some(point);
        Ok(())
    }
    /// Conclusions over BOTH original windows with the complete frozen
    /// denominator: every intended Master is reported by name. `MET`/`NOT_MET`
    /// summarise the frozen A6 rule; nothing here says PASS.
    pub fn finish(self) -> Result<serde_json::Value, String> {
        let c = self.contract;
        if self.points.len() < 3 {
            return Err("receipt39 complete bounded transcript required".into());
        }
        let mut windows = Vec::new();
        for w in &c.windows {
            let start = self.points.iter().rposition(|p| p.0 < w.start_daa).ok_or("receipt39 window beginning unobserved")?;
            let end = self.points.iter().rposition(|p| p.0 < w.end_daa).ok_or("receipt39 window end unobserved")?;
            let deadline = w.end_daa.checked_add(w.maximum_final_lag_daa).ok_or("receipt39 lag overflow")?;
            if !self.points.iter().any(|p| p.0 >= deadline) {
                return Err("receipt39 bounded Final tail unobserved".into());
            }
            let bins = ((w.end_daa - w.start_daa) / w.issuance_bin_daa) as usize;
            let (mut met_bins, mut operators, mut statuses, mut members) =
                (vec![0usize; bins], BTreeSet::new(), Vec::new(), BTreeMap::<String, (usize, usize)>::new());
            for m in &w.intended {
                let entry = members.entry(m.member.clone()).or_default();
                entry.1 += 1;
                #[allow(clippy::type_complexity)]
                let status = (|| -> Result<(String, Option<(u64, BTreeSet<Hash64>)>), String> {
                    let Some(o) = self.masters_final.get(&m.master) else { return Ok(("NOT_REGISTERED".into(), None)) };
                    let mut last = 0u64;
                    let mut ops = BTreeSet::new();
                    for (stage, comp) in o.components.iter().enumerate() {
                        let Some(comp) = comp else { return Ok((format!("COMPONENT_NOT_FUNDED:{stage}"), None)) };
                        let Some(committed) = comp.committed else { return Ok((format!("COMPONENT_NOT_COMMITTED:{stage}"), None)) };
                        if committed < m.issuance_not_before || committed >= m.issuance_deadline {
                            return Ok((format!("COMMIT_OUTSIDE_ISSUANCE:{stage}"), None));
                        }
                        if comp.terminal != "Final" {
                            return Ok((format!("COMPONENT_NOT_FINAL:{stage}:{}", comp.terminal), None));
                        }
                        let expected = &m.components[stage];
                        if comp.producer_operator != expected.producer_operator {
                            return Ok((format!("PRODUCER_OPERATOR_DIFFERS:{stage}"), None));
                        }
                        last = last.max(committed);
                        ops.insert(comp.producer_operator);
                    }
                    let Some((_, established, _)) = o.aggregate else { return Ok(("MASTER_FINAL_ABSENT".into(), None)) };
                    if established > last.saturating_add(w.maximum_final_lag_daa) || established > deadline {
                        return Ok(("MASTER_FINAL_STALLED".into(), None));
                    }
                    Ok(("FINAL_WITHIN_LAG".into(), Some((last, ops))))
                })()?;
                if let (_, Some((last, ops))) = &status {
                    let bin = ((last - w.start_daa) / w.issuance_bin_daa) as usize;
                    if bin < bins {
                        met_bins[bin] += 1;
                        operators.extend(ops.iter().copied());
                        entry.0 += 1;
                    }
                }
                statuses.push(serde_json::json!({"member": m.member, "master": m.master.to_string(), "status": status.0,
                    "first_registered_daa": self.first_observed.get(&m.master)}));
            }
            let first = &self.points[start].3;
            let last = &self.points[end].3;
            let queue_ok = last.live <= first.live && last.packets <= first.packets && last.owned_bytes <= first.owned_bytes;
            let met = statuses.iter().all(|s| s["status"] == "FINAL_WITHIN_LAG")
                && met_bins.iter().all(|n| *n >= w.minimum_commits_per_bin)
                && operators.len() >= c.minimum_independent_operators
                && queue_ok;
            windows.push(serde_json::json!({
                "start_daa": w.start_daa, "end_daa": w.end_daa,
                "issuance_bin_daa": w.issuance_bin_daa, "minimum_commits_per_bin": w.minimum_commits_per_bin,
                "maximum_final_lag_daa": w.maximum_final_lag_daa,
                "intended_masters": w.intended.len(),
                "final_within_lag": statuses.iter().filter(|s| s["status"] == "FINAL_WITHIN_LAG").count(),
                "per_master": statuses,
                "per_member": members.iter().map(|(k, (met, of))| serde_json::json!({"member": k, "met": met, "intended": of})).collect::<Vec<_>>(),
                "met_per_bin": met_bins,
                "independent_operators": operators.len(),
                "queue_start": {"live": first.live, "packets": first.packets, "owned_bytes": first.owned_bytes},
                "queue_end": {"live": last.live, "packets": last.packets, "owned_bytes": last.owned_bytes},
                "queue_non_growing": queue_ok,
                "a6_window": if met { "MET" } else { "NOT_MET" },
                "start_point": start, "end_point": end,
            }));
        }
        let both = windows.iter().all(|w| w["a6_window"] == "MET");
        let members_both = c
            .original_model_inventory_sets
            .keys()
            .map(|member| {
                let all = windows.iter().all(|w| {
                    w["per_member"].as_array().is_some_and(|a| {
                        a.iter().any(|m| {
                            m["member"] == *member && m["met"].as_u64() == m["intended"].as_u64() && m["intended"].as_u64() > Some(0)
                        })
                    })
                });
                serde_json::json!({"member": member, "both_windows": if all { "MET" } else { "NOT_MET" }})
            })
            .collect::<Vec<_>>();
        Ok(serde_json::json!({
            "schema": "misaka.rfc02.proof-derived-receipt-windows.v39",
            "contract_root": crate::runtime_pack::commit::hex(&c.root()?),
            "cohort_sha256": c.cohort_sha256,
            "receipt_window_daa": c.receipt_window_daa,
            "minimum_independent_operators": c.minimum_independent_operators,
            "original_model_inventory_sets": c.original_model_inventory_sets,
            "windows": windows,
            "a6_two_windows": if both { "MET" } else { "NOT_MET" },
            // A8 keeps the census denominators (D_all / D_complete) outside this
            // report; here every frozen member is listed whether or not it met.
            "a8_members": members_both,
            "points": self.points.len(),
            "cumulative": self.points.iter().map(|(daa, child, parent, q)| serde_json::json!({
                "daa": daa, "child": child.to_string(), "selected_parent": parent.to_string(),
                "live": q.live, "packets": q.packets, "owned_bytes": q.owned_bytes})).collect::<Vec<_>>(),
            "authority": "readonly measurement; not PASS; no receipt, coinbase or Final-weight right",
        }))
    }
}
