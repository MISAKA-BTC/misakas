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
struct CheckedQueueV39 {
    global: NativeQueueGlobalV30,
    physical: NativeCarriageGlobalV22,
    entries: BTreeMap<Digest, QueueOriginalV39>,
}
#[derive(Clone)]
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
