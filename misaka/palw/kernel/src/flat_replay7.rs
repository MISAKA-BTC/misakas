//! Versioned local replay limits for the complete v7 mixed-court graph. The v6
//! reference-cost model is reused as data, with an independently checked v7
//! scope and additional TopK scan, selected-value, authentication and retry
//! charges. No network-wide admission or hardware performance is implied.
use crate::{
    flat_court7::FlatContextV7,
    flat_detect5::{FlatPublicReadsV5, FlatReadV5},
    flat_detect7::{FlatFindingV7, FlatPublicReadsV7, FlatValueReadV7, TOPK_VALUE_MAX_BYTES_V7},
    flat_replay6::{FLAT_REPLAY_REPLY_BYTES_V6, FlatReplayEnvelopeV6, FlatReplayLimitsV6},
    flat_scope7::{FlatResourceScopeV7, check_flat_resource_scope_v7},
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
pub struct FlatReplayEnvelopeV7 {
    pub version: u16,
    pub scope: Digest,
    /// Frozen v6 reference-cost formulas, not a v5 scope or v6 admission.
    pub reference: FlatReplayEnvelopeV6,
    pub topk_scan_work_per_attempt: u128,
    pub topk_auxiliary_bytes: u128,
    pub selected_value_reads_per_attempt: u128,
    pub selected_reply_bytes_per_attempt: u128,
}
impl FlatReplayEnvelopeV7 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/flat-replay-envelope/v7", self)
    }
}
pub fn flat_replay_envelope_v7(
    program: &TirProgramV1,
    scope: &FlatResourceScopeV7,
    encoder: Option<&EncoderBindingV1>,
) -> Result<FlatReplayEnvelopeV7, DismissalV1> {
    check_flat_resource_scope_v7(program, scope.max_positions, encoder, scope)?;
    let mut reference = crate::flat_replay6::replay_costs_v6(program, scope.max_positions, scope.root(), encoder)
        .map_err(DismissalV1::NotAuthentic)?;
    // Authenticated selected leaves include typed metadata and hash paths.
    // Four wire bytes per byte covers hashing both children along each path,
    // payload/header framing and duplicate proofs. Reserve every attempt.
    reference.trace_hash_bytes_per_attempt += 4 * scope.topk_selected_reply_bytes_per_claim;
    reference.reply_bytes_per_attempt += scope.topk_selected_reply_bytes_per_claim;
    Ok(FlatReplayEnvelopeV7 {
        version: 7,
        scope: scope.root(),
        reference,
        topk_scan_work_per_attempt: scope.topk_selection_work_per_claim,
        topk_auxiliary_bytes: scope.topk_selection_aux_bytes,
        selected_value_reads_per_attempt: scope.topk_selected_value_reads_per_claim,
        selected_reply_bytes_per_attempt: scope.topk_selected_reply_bytes_per_claim,
    })
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlatReplayLimitsV7 {
    pub reference: FlatReplayLimitsV6,
    pub topk_scan_work: u128,
    pub topk_auxiliary_bytes: u128,
    pub selected_value_reads: u128,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FlatReplayResultV7 {
    Finding(FlatFindingV7),
    Limit(&'static str),
    Inconsistent(String),
}
pub struct FlatReplaySessionV7 {
    limits: FlatReplayLimitsV7,
    pub attempts: u32,
    pub parameter_hash_bytes: u128,
    pub trace_hash_bytes: u128,
    pub topk_scan_work: u128,
    pub node_reads: Cell<u128>,
    pub hash_reads: Cell<u128>,
    pub selected_value_reads: Cell<u128>,
    pub reply_bytes: Cell<u128>,
}
impl FlatReplaySessionV7 {
    pub fn new(limits: FlatReplayLimitsV7) -> Self {
        Self {
            limits,
            attempts: 0,
            parameter_hash_bytes: 0,
            trace_hash_bytes: 0,
            topk_scan_work: 0,
            node_reads: Cell::new(0),
            hash_reads: Cell::new(0),
            selected_value_reads: Cell::new(0),
            reply_bytes: Cell::new(0),
        }
    }
    pub fn check(
        &mut self,
        c: &FlatContextV7<'_>,
        declared: &FlatReplayEnvelopeV7,
        public: &dyn FlatPublicReadsV7,
        artifact: &dyn Fn(u16, Option<u16>) -> Option<Tensor>,
        tokens: &[u32],
    ) -> FlatReplayResultV7 {
        if let Err(e) = c.prepare() {
            return FlatReplayResultV7::Inconsistent(format!("{e:?}"));
        }
        let e = match flat_replay_envelope_v7(c.base.claim.program, c.scope, c.base.claim.encoder.as_ref()) {
            Ok(e) => e,
            Err(e) => return FlatReplayResultV7::Inconsistent(format!("{e:?}")),
        };
        if e != *declared {
            return FlatReplayResultV7::Inconsistent("v7 replay declaration differs from independent derivation".into());
        }
        for (need, cap, name) in [
            (e.reference.position_tensor_bytes, self.limits.reference.position_tensor_bytes, "position tensor bytes"),
            (e.reference.largest_operand_tensor_bytes, self.limits.reference.operand_tensor_bytes, "operand tensor bytes"),
            (e.reference.history_rows_tensor_bytes, self.limits.reference.history_tensor_bytes, "history tensor bytes"),
            (e.topk_auxiliary_bytes, self.limits.topk_auxiliary_bytes, "TopK auxiliary bytes"),
        ] {
            if need > cap {
                return FlatReplayResultV7::Limit(name);
            }
        }
        if self.attempts >= self.limits.reference.attempts {
            return FlatReplayResultV7::Limit("attempts");
        }
        let mut additions = Vec::with_capacity(3);
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
        ] {
            let Some(total) = used.checked_add(need).filter(|v| *v <= cap) else { return FlatReplayResultV7::Limit(name) };
            additions.push(total);
        }
        if self.node_reads.get() >= self.limits.reference.node_reads {
            return FlatReplayResultV7::Limit("node reads");
        }
        if self.reply_bytes.get().checked_add(FLAT_REPLAY_REPLY_BYTES_V6).is_none_or(|v| v > self.limits.reference.reply_bytes) {
            return FlatReplayResultV7::Limit("reply bytes");
        }
        // Reserve the whole reference hashing and TopK scan attempt before any
        // public/parameter callback. Demand, bad artifact and retry spend it.
        self.attempts += 1;
        self.parameter_hash_bytes = additions[0];
        self.trace_hash_bytes = additions[1];
        self.topk_scan_work = additions[2];
        struct Bounded<'a> {
            session: &'a FlatReplaySessionV7,
            public: &'a dyn FlatPublicReadsV7,
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
        impl FlatPublicReadsV7 for Bounded<'_> {
            fn value(&self, r: FlatValueReadV7) -> Option<Vec<u8>> {
                if !self.take(
                    &self.session.selected_value_reads,
                    self.session.limits.selected_value_reads,
                    TOPK_VALUE_MAX_BYTES_V7 as u128,
                    "selected value reads",
                ) {
                    return None;
                }
                let b = self.public.value(r)?;
                if b.len() as u64 > TOPK_VALUE_MAX_BYTES_V7 {
                    return None;
                }
                Some(b)
            }
        }
        let bounded = Bounded { session: self, public, hit: Cell::new(None) };
        let mut scan =
            TopKScanBudgetV7 { remaining_work: e.topk_scan_work_per_attempt, auxiliary_ceiling: self.limits.topk_auxiliary_bytes };
        let finding = crate::flat_detect7::check_flat_claim_v7(c, &bounded, artifact, tokens, &mut scan);
        match bounded.hit.get() {
            Some(d) => FlatReplayResultV7::Limit(d),
            None => FlatReplayResultV7::Finding(finding),
        }
    }
}
