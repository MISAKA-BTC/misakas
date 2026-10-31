//! Retained historical EVM projection. Public RPC/simulation is not wired.
//! Historical gate compatibility only; this does not activate model examination.
use super::*;
use crate::evm::model_market::*;
use crate::palw_native_resource_v1::{NativeCoverageV1, NativeRetainedV1, btree_node_envelope_bytes_v1, reserve_native_requests_v1};
use std::{collections::BTreeMap, mem::size_of, sync::Arc};

/// The only extras consulted by the legacy market-read refusal. Contents of
/// registry genesis works and unrelated transition extras are not consulted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NativeEvmMarketGateInputsV2 {
    pub audit_active: bool,
    pub virtual_active: bool,
    pub registry_present: bool,
    pub work_target_active: bool,
}
impl NativeEvmMarketGateInputsV2 {
    pub fn from_extras_v2(extras: &PalwTransitionExtrasV1) -> Self {
        Self {
            audit_active: extras.audit_2026_09_23_active,
            virtual_active: extras.model_virtual_v1.is_some(),
            registry_present: extras.model_registry.is_some(),
            work_target_active: extras.work_target_active,
        }
    }
}
/// Allocation-free reason, shared with the legacy gate when this draft is
/// registered. Error formatting is deferred to a legacy error-returning caller.
pub(super) enum EvmMarketRefusalV2<'a> {
    MissingClass,
    NotActive,
    Lifecycle { state: &'a crate::palw_model_registry_v1::PalwModelLifecycleV1, trading: bool },
    NoRegistryRow { trading: bool },
}
impl EvmMarketRefusalV2<'_> {
    pub(super) fn into_legacy_error_v2(self, class: Hash64) -> PalwStateV2Error {
        match self {
            Self::MissingClass => PalwStateV2Error::MissingClass(class),
            Self::NotActive => PalwStateV2Error::ModelClassNotActive(class),
            Self::Lifecycle { state, trading } => {
                let state = format!("{state:?}");
                if trading {
                    PalwStateV2Error::ModelClassNotTrading { class, state }
                } else {
                    PalwStateV2Error::ModelClassNotEligible { class, state }
                }
            }
            Self::NoRegistryRow { trading } => {
                let state = "no row".to_string();
                if trading {
                    PalwStateV2Error::ModelClassNotTrading { class, state }
                } else {
                    PalwStateV2Error::ModelClassNotEligible { class, state }
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeEvmViewQuoteV2 {
    pub persistent_bytes: u64,
    pub scratch_bytes: u64,
}
fn overflow() -> NativeCoverageV1 {
    NativeCoverageV1::overflow::<PalwEvmViewV1>("historical EVM view")
}
fn add(a: u64, b: u64) -> Result<u64, NativeCoverageV1> {
    a.checked_add(b).ok_or_else(overflow)
}
fn bytes<T>(n: usize) -> Result<u64, NativeCoverageV1> {
    (n as u64).checked_mul(size_of::<T>() as u64).ok_or_else(overflow)
}
fn nodes<K, V>(n: usize) -> Result<u64, NativeCoverageV1> {
    (n as u64).checked_mul(btree_node_envelope_bytes_v1::<K, V>() as u64).ok_or_else(overflow)
}
impl PalwChainStateV2 {
    /// Order and error reason are the legacy fold's: virtual trading first
    /// checks class presence/status, then registry; ordinary admission only
    /// checks lifecycle when the audit and registry are in force.
    pub(super) fn evm_market_refusal_v2(
        &self,
        base: Hash64,
        g: NativeEvmMarketGateInputsV2,
        class: &Hash64,
    ) -> Option<EvmMarketRefusalV2<'_>> {
        use crate::palw_model_registry_v1::PalwModelLifecycleV1 as L;
        if g.virtual_active {
            let Some(row) = self.classes.get(class) else { return Some(EvmMarketRefusalV2::MissingClass) };
            if !matches!(row.status, PalwClassStatusV2::Active) {
                return Some(EvmMarketRefusalV2::NotActive);
            }
        } else if !g.audit_active {
            return None;
        }
        if !g.registry_present || *class == base {
            return None;
        }
        match self.model_lifecycles.get(class) {
            Some(row) if if g.virtual_active { !matches!(row.state, L::Active) } else { !row.state.admits_claims() } => {
                Some(EvmMarketRefusalV2::Lifecycle { state: &row.state, trading: g.virtual_active })
            }
            None if g.work_target_active => Some(EvmMarketRefusalV2::NoRegistryRow { trading: g.virtual_active }),
            _ => None,
        }
    }
    /// Logical requested allocations, not RSS or an allocator safety campaign.
    /// Counts borrow actual source tables and do not allocate quote indexes.
    pub fn evm_view_quote_v2(&self) -> Result<NativeEvmViewQuoteV2, NativeCoverageV1> {
        if usize::BITS != 64 {
            return Err(NativeCoverageV1::unsupported::<PalwEvmViewV1>("view map envelopes", "64-bit pinned BTree layout required"));
        }
        let c = self.classes.len();
        let l = add(c as u64, self.model_lines.len() as u64)? as usize;
        let v = add(c as u64, self.model_versions.len() as u64)? as usize;
        let p = self.model_proposals.len();
        let e = self.model_evaluations.len();
        // Arc<NativeRetained<View>> owns the payload and final ticket together.
        let mut persistent = bytes::<NativeRetainedV1<PalwEvmViewV1>>(1)?;
        persistent = add(persistent, bytes::<usize>(3)?)?;
        persistent = add(persistent, bytes::<(Hash64, PalwEvmClassRowV1)>(c)?)?;
        // Each root belongs to one class/line. Dedup can only reduce this bound.
        persistent = add(persistent, bytes::<Hash64>(v)?)?;
        persistent = add(persistent, bytes::<(Hash64, PalwEvmLineRowV1)>(l)?)?;
        for line in self.model_lines.values() {
            persistent = add(persistent, bytes::<u8>(line.name.len())?)?;
        }
        persistent = add(persistent, nodes::<(Hash64, u32), PalwEvmVersionRowV1>(v)?)?;
        persistent = add(persistent, nodes::<Hash64, Vec<PalwEvmProposalRowV1>>(p)?)?;
        persistent = add(persistent, bytes::<PalwEvmProposalRowV1>(p)?)?;
        persistent = add(persistent, nodes::<(Hash64, u32), Vec<PalwEvmEvaluationRowV1>>(e)?)?;
        persistent = add(persistent, bytes::<PalwEvmEvaluationRowV1>(e)?)?;
        persistent = add(persistent, nodes::<Hash64, crate::palw_model_market_v1::PalwModelMarketV1>(self.model_markets.len())?)?;
        persistent = add(persistent, nodes::<(Hash64, Hash64), u64>(self.model_positions.len())?)?;
        persistent = add(persistent, nodes::<Hash64, ()>(c)?)?;
        // The intermediate line tree coexists with the final line Vec. Group
        // cardinality indexes coexist with final proposal/evaluation arrays.
        let mut scratch = nodes::<Hash64, PalwEvmLineRowV1>(l)?;
        scratch = add(scratch, nodes::<Hash64, usize>(p)?)?;
        scratch = add(scratch, nodes::<(Hash64, u32), usize>(e)?)?;
        Ok(NativeEvmViewQuoteV2 { persistent_bytes: persistent, scratch_bytes: scratch })
    }
    fn evm_view_owned_from_retained_input_v2(
        &self,
        chain_id: u64,
        base: Hash64,
        gate: NativeEvmMarketGateInputsV2,
    ) -> Result<Arc<NativeRetainedV1<PalwEvmViewV1>>, NativeCoverageV1> {
        let q = self.evm_view_quote_v2()?;
        let mut construction = reserve_native_requests_v1(add(q.persistent_bytes, q.scratch_bytes)?)?;
        #[cfg(test)]
        BUILT.with(|count| count.set(count.get() + 1));
        let mut view = self.evm_view_reserved_payload_v2(chain_id, base);
        for class in self.classes.keys() {
            if self.evm_market_refusal_v2(base, gate, class).is_some() {
                view.market_refused_classes.insert(*class);
            }
        }
        let owner = construction.as_mut().map(|lease| lease.split_memory_v1(q.persistent_bytes)).transpose()?;
        drop(construction);
        Ok(Arc::new(NativeRetainedV1::from_owner_reserved_v1(view, owner)))
    }
    fn evm_roots_reserved_v2(&self, class_id: &Hash64, daa: u64) -> Vec<Hash64> {
        let Some(class) = self.classes.get(class_id) else { return Vec::new() };
        let founding = !self.model_lines.contains_key(class_id);
        let count = self
            .model_lines
            .iter()
            .filter(|(_, line)| line.class_id == *class_id && line.retired_daa.is_none_or(|d| daa < d))
            .map(|(line, _)| self.model_versions.range((*line, 0)..=(*line, u32::MAX)).filter(|(_, v)| v.in_force_at(daa)).count())
            .sum::<usize>();
        let mut roots = Vec::with_capacity(count + usize::from(founding));
        if founding {
            roots.push(class.artifact_root);
        }
        for (id, line) in self.model_lines.iter().filter(|(_, line)| line.class_id == *class_id) {
            if line.retired_daa.is_some_and(|d| daa >= d) {
                continue;
            }
            for (_, version) in self.model_versions.range((*id, 0)..=(*id, u32::MAX)) {
                if version.in_force_at(daa) && !roots.contains(&version.root) {
                    roots.push(version.root);
                }
            }
        }
        roots
    }
    fn evm_view_reserved_payload_v2(&self, chain_id: u64, base_class_id: Hash64) -> crate::evm::model_market::PalwEvmViewV1 {
        use crate::evm::model_market::*;
        use crate::palw_model_lines_v1::{PalwModelLineStatusV1, PalwVersionStatusV1};
        let daa = self.last_point().map(|p| p.daa_score).unwrap_or(0);
        let payload_of = |bond: Option<PalwBondKeyV2>| bond.and_then(|b| self.bonds.get(&b)).map(|b| b.payout_payload);
        let mut classes = Vec::with_capacity(self.classes.len());
        for (id, class) in self.classes.iter() {
            let status = match class.status {
                PalwClassStatusV2::Active => 0,
                PalwClassStatusV2::Frozen { .. } => 1,
                PalwClassStatusV2::Registered { .. } => 2,
                PalwClassStatusV2::Dormant { .. } => 3,
            };
            let row = PalwEvmClassRowV1 {
                status,
                share_permille: self.class_shares.get(id).copied().unwrap_or(0),
                budget_blocks: self.epoch_budgets.as_ref().and_then(|b| b.budget_blocks.get(id).copied()).unwrap_or(0),
                canonical_leaves: class.pwu_rule.canonical_leaves_v1(),
                is_base: *id == base_class_id,
                registrant_payload: payload_of(class.registrant_bond),
                registered_daa: class.registered_daa,
                certified_attempt: self.class_shares.contains_key(id),
                certified_fp: self.fp_certified_classes.contains_key(id),
                roots_in_force: self.evm_roots_reserved_v2(id, daa),
            };
            classes.push((*id, row));
        }
        let mut lines: BTreeMap<Hash64, PalwEvmLineRowV1> = BTreeMap::new();
        let mut versions: BTreeMap<(Hash64, u32), PalwEvmVersionRowV1> = BTreeMap::new();
        let line_row = |line: &crate::palw_model_lines_v1::PalwModelLineV1| {
            let mut name = Vec::with_capacity(line.name.len());
            name.extend_from_slice(&line.name);
            PalwEvmLineRowV1 {
                class_id: line.class_id,
                owner_payload: payload_of(line.owner),
                developer_payload: payload_of(line.developer_bond()),
                maintainer_payload: payload_of(line.maintainer_bond()),
                name,
                founded_daa: line.founded_daa,
                current: line.current,
                versions_published: line.versions_published,
                preview_count: line.previews.len() as u32,
                contributor_permille_of_leg: line.contributor_permille_of_leg,
                status: match line.status {
                    PalwModelLineStatusV1::Active => 0,
                    PalwModelLineStatusV1::Retired => 1,
                },
            }
        };
        let version_row = |v: &crate::palw_model_lines_v1::PalwModelVersionV1| {
            let (status, until_daa) = match v.status {
                PalwVersionStatusV1::Current => (0, 0),
                PalwVersionStatusV1::Preview => (1, 0),
                PalwVersionStatusV1::Superseded { until_daa } => (2, until_daa),
                PalwVersionStatusV1::Withdrawn => (3, 0),
            };
            PalwEvmVersionRowV1 {
                root: v.root,
                parent: v.parent,
                adopted_from: v.adopted_from,
                runtime_hash: v.runtime_hash,
                dataset_commitment: v.dataset_commitment,
                training_config_hash: v.training_config_hash,
                notes_hash: v.notes_hash,
                published_daa: v.published_daa,
                published_by_payload: payload_of(v.published_by),
                status,
                until_daa,
                attempt_claims: v.usage.attempt_claims,
                fp_claims: v.usage.fp_claims,
                work_leaves: v.usage.work_leaves,
                first_used_daa: v.usage.first_used_daa,
                last_used_daa: v.usage.last_used_daa,
            }
        };
        for id in self.classes.keys() {
            if *id == base_class_id {
                continue;
            }
            if !self.model_lines.contains_key(id)
                && let Some(line) = self.model_line_or_founding(id)
                && let Some(v1) = self.model_version(id, 1)
            {
                lines.insert(*id, line_row(&line));
                versions.insert((*id, 1), version_row(&v1));
            }
        }
        for (id, line) in self.model_lines.iter() {
            lines.insert(*id, line_row(line));
            for ((_, n), v) in self.model_versions.range((*id, 0)..=(*id, u32::MAX)) {
                versions.insert((*id, *n), version_row(v));
            }
        }
        let mut proposal_counts = BTreeMap::<Hash64, usize>::new();
        for p in self.model_proposals.values() {
            *proposal_counts.entry(p.line_id).or_default() += 1;
        }
        let mut evaluation_counts = BTreeMap::<(Hash64, u32), usize>::new();
        for (line, version, _) in self.model_evaluations.keys() {
            *evaluation_counts.entry((*line, *version)).or_default() += 1;
        }
        let mut proposals: BTreeMap<Hash64, Vec<PalwEvmProposalRowV1>> = BTreeMap::new();
        for (pid, p) in self.model_proposals.iter() {
            proposals.entry(p.line_id).or_insert_with(|| Vec::with_capacity(proposal_counts[&p.line_id])).push(PalwEvmProposalRowV1 {
                proposal_id: *pid,
                root: p.root,
                note_hash: p.note_hash,
                by_payload: payload_of(Some(p.by)),
                posted_daa: p.posted_daa,
                adopted_in: p.adopted_in,
            });
        }
        let mut evaluations: BTreeMap<(Hash64, u32), Vec<PalwEvmEvaluationRowV1>> = BTreeMap::new();
        for ((l, n, by), e) in self.model_evaluations.iter() {
            evaluations.entry((*l, *n)).or_insert_with(|| Vec::with_capacity(evaluation_counts[&(*l, *n)])).push(
                PalwEvmEvaluationRowV1 {
                    evaluator_id: e.evaluator_id,
                    score_permille: e.score_permille,
                    report_hash: e.report_hash,
                    by_payload: payload_of(Some(*by)),
                    posted_daa: e.posted_daa,
                    is_lines_own: e.is_lines_own,
                },
            );
        }
        let mut line_rows = Vec::with_capacity(lines.len());
        line_rows.extend(lines);
        PalwEvmViewV1 {
            chain_daa: daa,
            chain_id,
            classes,
            lines: line_rows,
            versions,
            proposals,
            evaluations,
            markets: self.model_markets.clone(),
            positions: self.model_positions.clone(),
            // P-B3: filled by the caller that holds the block's extras
            // (`palw_evm_market_refused_classes_v1`), and only past the audit fence.
            market_refused_classes: Default::default(),
        }
    }
}

/// The construction entry requires a retained input, keeping its ticket live
/// while output and scratch are separately reserved. A bare state does not gain
/// this new owned construction method.
impl NativeRetainedV1<PalwChainStateV2> {
    pub fn evm_view_owned_v2(
        &self,
        chain_id: u64,
        base: Hash64,
        gate: NativeEvmMarketGateInputsV2,
    ) -> Result<Arc<NativeRetainedV1<PalwEvmViewV1>>, NativeCoverageV1> {
        self.evm_view_owned_from_retained_input_v2(chain_id, base, gate)
    }
}

#[cfg(test)]
thread_local! { static BUILT:std::cell::Cell<u64>=const{std::cell::Cell::new(0)}; }
#[cfg(test)]
#[path = "evm_view_owned_tests_v2.rs"]
mod tests;
