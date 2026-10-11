//! Versioned local replay limits for the complete v9 mixed-court graph. The v6
//! reference-cost model is reused as data, with an independently checked v9
//! scope and additional TopK scan, selected-value, authentication and retry
//! charges. No network-wide admission or hardware performance is implied.
use crate::{
    flat_court9::FlatContextV9,
    flat_detect5::{FlatPublicReadsV5, FlatReadV5},
    flat_detect9::{
        FlatFindingV9, FlatHeaderReadV9, FlatPublicReadsV9, FlatValueReadV9, TYPED_HEADER_MAX_BYTES_V9, TYPED_VALUE_MAX_BYTES_V9,
    },
    flat_replay6::{FLAT_REPLAY_REPLY_BYTES_V6, FlatReplayEnvelopeV6, FlatReplayLimitsV6},
    flat_scope9::{FlatResourceScopeV9, check_flat_resource_scope_v9},
    flat_topk7::TopKScanBudgetV7,
    hash::{Digest, object_id},
    merkle5::FlatLeafHashV5,
    seg::NodeOpeningV1,
    seg_encoder::EncoderBindingV1,
    verify::DismissalV1,
};
use borsh::BorshSerialize;
use misaka_palw_tir::{Tensor, program::TirProgramV1};
use std::cell::Cell;

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct FlatReplayEnvelopeV9 {
    pub version: u16,
    pub scope: Digest,
    /// Frozen v6 reference-cost formulas, not a v5 scope or v6 admission.
    pub reference: FlatReplayEnvelopeV6,
    pub topk_scan_work_per_attempt: u128,
    pub topk_auxiliary_bytes: u128,
    pub selected_value_reads_per_attempt: u128,
    pub selected_reply_bytes_per_attempt: u128,
    pub header_reads_per_attempt: u128,
    pub scope_checker_work_per_attempt: u128,
}
impl FlatReplayEnvelopeV9 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/flat-replay-envelope/v9", self)
    }
}
pub fn flat_replay_envelope_v9(
    program: &TirProgramV1,
    scope: &FlatResourceScopeV9,
    encoder: Option<&EncoderBindingV1>,
) -> Result<FlatReplayEnvelopeV9, DismissalV1> {
    check_flat_resource_scope_v9(program, scope.max_positions, encoder, scope)?;
    replay_envelope_from_verified_scope_v9(program, scope, scope.root(), encoder)
}
/// Only freshly, independently derived scope metadata enters this helper from
/// native candidate preparation. Public replay/checkers always rederive scope.
pub(crate) fn replay_envelope_from_verified_scope_v9(
    program: &TirProgramV1,
    scope: &FlatResourceScopeV9,
    scope_root: Digest,
    encoder: Option<&EncoderBindingV1>,
) -> Result<FlatReplayEnvelopeV9, DismissalV1> {
    let mut reference = crate::flat_replay6::replay_costs_v6(program, scope.max_positions, scope_root, encoder)
        .map_err(DismissalV1::NotAuthentic)?;
    let pr = crate::element::PreparedV1::without_scope(program, encoder).map_err(DismissalV1::NotAuthentic)?;
    let (mut topk_work, mut aux, mut reads) = (0u128, 0u128, 63u128);
    for (s, (b, _)) in pr.w.occurrences.iter().enumerate() {
        for (n, node) in program.blocks[*b as usize].nodes.iter().enumerate() {
            if !matches!(node.prim, misaka_palw_tir::Prim::TopK { .. }) {
                continue;
            }
            let (mut work, mut relation_reads) = (0, 0);
            for q in [0, scope.max_positions - 1] {
                if !crate::flat_scope7::point_relation(&pr, q, s as u16, n as u16)? {
                    continue;
                }
                let t = crate::flat_topk7::relation_scope_v7(program, &pr, scope.max_positions, q, s as u16, n as u16, true)?;
                work = work.max(t.selection_work);
                aux = aux.max(t.selection_aux_bytes);
                relation_reads = relation_reads.max(t.rows as u128 * (t.k as u128 + 3));
            }
            topk_work = topk_work
                .checked_add(work * scope.max_positions as u128)
                .ok_or_else(|| DismissalV1::NotAuthentic("v9 TopK work overflow".into()))?;
            reads = reads.max(relation_reads);
        }
    }
    let typed_reply = reads * TYPED_VALUE_MAX_BYTES_V9 as u128;
    let header_reply = TYPED_HEADER_MAX_BYTES_V9 as u128;
    // At most one first divergent relation/header is inspected per attempt.
    // Reserve worst typed leaf reads even if the first acquired reply is absent.
    reference.trace_hash_bytes_per_attempt += 4 * (typed_reply + header_reply);
    reference.reply_bytes_per_attempt += typed_reply + header_reply;
    Ok(FlatReplayEnvelopeV9 {
        version: 9,
        scope: scope_root,
        reference,
        topk_scan_work_per_attempt: topk_work,
        topk_auxiliary_bytes: aux,
        selected_value_reads_per_attempt: reads,
        selected_reply_bytes_per_attempt: typed_reply,
        header_reads_per_attempt: 1,
        scope_checker_work_per_attempt: scope.metadata_derivation_work * 8,
    })
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlatReplayLimitsV9 {
    pub reference: FlatReplayLimitsV6,
    pub topk_scan_work: u128,
    pub topk_auxiliary_bytes: u128,
    pub selected_value_reads: u128,
    pub header_reads: u128,
    pub scope_checker_work: u128,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FlatReplayResultV9 {
    Finding(FlatFindingV9),
    Limit(&'static str),
    Inconsistent(String),
}
pub struct FlatReplaySessionV9 {
    limits: FlatReplayLimitsV9,
    pub attempts: u32,
    pub parameter_hash_bytes: u128,
    pub trace_hash_bytes: u128,
    pub topk_scan_work: u128,
    pub scope_checker_work: u128,
    pub header_reads: Cell<u128>,
    pub node_reads: Cell<u128>,
    pub hash_reads: Cell<u128>,
    pub selected_value_reads: Cell<u128>,
    pub reply_bytes: Cell<u128>,
}
impl FlatReplaySessionV9 {
    pub fn new(limits: FlatReplayLimitsV9) -> Self {
        Self {
            limits,
            attempts: 0,
            parameter_hash_bytes: 0,
            trace_hash_bytes: 0,
            topk_scan_work: 0,
            scope_checker_work: 0,
            header_reads: Cell::new(0),
            node_reads: Cell::new(0),
            hash_reads: Cell::new(0),
            selected_value_reads: Cell::new(0),
            reply_bytes: Cell::new(0),
        }
    }
    pub fn check(
        &mut self,
        c: &FlatContextV9<'_>,
        declared: &FlatReplayEnvelopeV9,
        public: &dyn FlatPublicReadsV9,
        artifact: &dyn Fn(u16, Option<u16>) -> Option<Tensor>,
        tokens: &[u32],
    ) -> FlatReplayResultV9 {
        if let Err(e) = c.prepare() {
            return FlatReplayResultV9::Inconsistent(format!("{e:?}"));
        }
        let e = match flat_replay_envelope_v9(c.base.claim.program, c.scope, c.base.claim.encoder.as_ref()) {
            Ok(e) => e,
            Err(e) => return FlatReplayResultV9::Inconsistent(format!("{e:?}")),
        };
        if e != *declared {
            return FlatReplayResultV9::Inconsistent("v9 replay declaration differs from independent derivation".into());
        }
        for (need, cap, name) in [
            (e.reference.position_tensor_bytes, self.limits.reference.position_tensor_bytes, "position tensor bytes"),
            (e.reference.largest_operand_tensor_bytes, self.limits.reference.operand_tensor_bytes, "operand tensor bytes"),
            (e.reference.history_rows_tensor_bytes, self.limits.reference.history_tensor_bytes, "history tensor bytes"),
            (e.topk_auxiliary_bytes, self.limits.topk_auxiliary_bytes, "TopK auxiliary bytes"),
        ] {
            if need > cap {
                return FlatReplayResultV9::Limit(name);
            }
        }
        if self.attempts >= self.limits.reference.attempts {
            return FlatReplayResultV9::Limit("attempts");
        }
        let mut additions = Vec::with_capacity(4);
        for (used, need, cap, name) in [
            (
                self.parameter_hash_bytes,
                e.reference.parameter_hash_bytes_per_attempt,
                self.limits.reference.parameter_hash_bytes,
                "parameter hash bytes",
            ),
            (
                self.trace_hash_bytes,
                e.reference.trace_hash_bytes_per_attempt,
                self.limits.reference.trace_hash_bytes,
                "trace hash bytes",
            ),
            (self.topk_scan_work, e.topk_scan_work_per_attempt, self.limits.topk_scan_work, "TopK scan work"),
            (self.scope_checker_work, e.scope_checker_work_per_attempt, self.limits.scope_checker_work, "scope checker work"),
        ] {
            let Some(total) = used.checked_add(need).filter(|v| *v <= cap) else { return FlatReplayResultV9::Limit(name) };
            additions.push(total);
        }
        if self.node_reads.get() >= self.limits.reference.node_reads {
            return FlatReplayResultV9::Limit("node reads");
        }
        if self.reply_bytes.get().checked_add(FLAT_REPLAY_REPLY_BYTES_V6).is_none_or(|v| v > self.limits.reference.reply_bytes) {
            return FlatReplayResultV9::Limit("reply bytes");
        }
        // Reserve the whole reference hashing and TopK scan attempt before any
        // public/parameter callback. Demand, bad artifact and retry spend it.
        self.attempts += 1;
        self.parameter_hash_bytes = additions[0];
        self.trace_hash_bytes = additions[1];
        self.topk_scan_work = additions[2];
        self.scope_checker_work = additions[3];
        struct Bounded<'a> {
            session: &'a FlatReplaySessionV9,
            public: &'a dyn FlatPublicReadsV9,
            hit: Cell<Option<&'static str>>,
        }
        impl Bounded<'_> {
            fn take(&self, count: &Cell<u128>, cap: u128, bytes: u128, name: &'static str) -> bool {
                let next = self.session.reply_bytes.get().checked_add(bytes);
                if count.get() >= cap {
                    self.hit.set(Some(name));
                    return false;
                }
                let Some(next) = next.filter(|v| *v <= self.session.limits.reference.reply_bytes) else {
                    self.hit.set(Some("reply bytes"));
                    return false;
                };
                count.set(count.get() + 1);
                self.session.reply_bytes.set(next);
                true
            }
        }
        impl FlatPublicReadsV5 for Bounded<'_> {
            fn node(&self, u: FlatReadV5) -> Option<NodeOpeningV1> {
                self.take(&self.session.node_reads, self.session.limits.reference.node_reads, FLAT_REPLAY_REPLY_BYTES_V6, "node reads")
                    .then(|| self.public.node(u))
                    .flatten()
            }
            fn hash(&self, u: FlatReadV5) -> Option<FlatLeafHashV5> {
                self.take(&self.session.hash_reads, self.session.limits.reference.hash_reads, FLAT_REPLAY_REPLY_BYTES_V6, "hash reads")
                    .then(|| self.public.hash(u))
                    .flatten()
            }
        }
        impl FlatPublicReadsV9 for Bounded<'_> {
            fn header(&self, r: FlatHeaderReadV9) -> Option<Vec<u8>> {
                if !self.take(
                    &self.session.header_reads,
                    self.session.limits.header_reads,
                    TYPED_HEADER_MAX_BYTES_V9 as u128,
                    "header reads",
                ) {
                    return None;
                }
                let b = self.public.header(r)?;
                (b.len() as u64 <= TYPED_HEADER_MAX_BYTES_V9).then_some(b)
            }
            fn value(&self, r: FlatValueReadV9) -> Option<Vec<u8>> {
                if !self.take(
                    &self.session.selected_value_reads,
                    self.session.limits.selected_value_reads,
                    TYPED_VALUE_MAX_BYTES_V9 as u128,
                    "selected value reads",
                ) {
                    return None;
                }
                let b = self.public.value(r)?;
                if b.len() as u64 > TYPED_VALUE_MAX_BYTES_V9 {
                    return None;
                }
                Some(b)
            }
        }
        let bounded = Bounded { session: self, public, hit: Cell::new(None) };
        let mut scan =
            TopKScanBudgetV7 { remaining_work: e.topk_scan_work_per_attempt, auxiliary_ceiling: self.limits.topk_auxiliary_bytes };
        let finding = crate::flat_detect9::check_flat_claim_v9(c, &bounded, artifact, tokens, &mut scan);
        match bounded.hit.get() {
            Some(d) => FlatReplayResultV9::Limit(d),
            None => FlatReplayResultV9::Finding(finding),
        }
    }
}
