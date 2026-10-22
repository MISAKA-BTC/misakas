//! **Liveness hotfix (L-LC-02 / L-COURT-01): the H-1 gate's guard — NODE-LOCAL admission policy, never a block rule.**
//!
//! The mempool's H-1 gate rehearses every gated carrier on the tip: the acceptance layer (for a completing split-close chunk that is
//! the assembly, digest, decode and adjudication of up to 2.7 MB) and then the object's own fold arm, whose `TransitionBuilder`
//! deep-copies the whole PALW state. A refusal is never mined, so no fee is paid and the funding UTXO stays unspent: one UTXO funded
//! unlimited rehearsals, the same refused completion could be resent for its group's whole assembly window, and the relay flow never
//! penalised the peer. The guard bounds that work without changing the gate's decision:
//!
//! * **(b) memory of verdicts.** The gate's verdict is a function of the tip (the sink the reader state stands at), the point it
//!   judges at (DAA, blue score) and the object, so it is remembered per `(sink, daa, blue, object digest)` (a bounded map) and a
//!   resent object — under any txid — is not re-rehearsed on the same tip. A refused txid is remembered per tip; an object refused
//!   for a reason that holds at every tip ([`PalwH1RefusalClassV1::Invalid`]) is remembered across tips for
//!   [`PALW_H1_INVALID_MEMORY_DAA_V1`] (longer than a split close's assembly window). A node-local resource refusal (typed RAM
//!   admission) is never remembered.
//! * **(c) a work budget.** Every rehearsal the memories do not answer takes units from a global token bucket (a completing close
//!   weighs its chunks, an objective offence its possible prompt recompute); over budget the carrier is refused unjudged, as a
//!   timing refusal (`PalwH1CarrierThrottled`), which no peer is penalised for. The per-peer half lives in the relay flow.
//! * **(a) the class of a refusal**, for the relay flow's penalty: [`PalwH1RefusalClassV1::Invalid`] only for refusals no honest node
//!   relays at any tip — a completing chunk whose close does not adjudicate to its declaration (its bytes are pinned in rooted state
//!   and its digests on its face), and a possession proof whose shape, challenge opening or immutable root is wrong
//!   (`palw_h1_precheck_v1`). Everything else (a stale tip, a span one block ahead, a missing claim or row, a signature under a
//!   bond the tip has not registered yet) is [`PalwH1RefusalClassV1::Timing`]. Within one DAA of any scheduled fence every refusal
//!   is Timing, so a peer on the other side of a flag day is never penalised.
//!
//! The template's re-ask and the mempool sweep read the same verdict memory (exact: same inputs, same answer) but never the budget
//! or the txid memory, so an admitted carrier is judged exactly as before.

use super::*;
use std::collections::{HashMap, VecDeque};
use std::hash::Hash;
use std::time::Instant;

/// Verdicts remembered (all tips together; the oldest leaves first).
pub(crate) const PALW_H1_VERDICT_MEMO_CAP_V1: usize = 8_192;
/// Refused txids remembered.
pub(crate) const PALW_H1_REFUSED_TXIDS_CAP_V1: usize = 8_192;
/// Objects refused as invalid at every tip, remembered across tips.
pub(crate) const PALW_H1_INVALID_OBJECTS_CAP_V1: usize = 4_096;
/// How long (in DAA) an every-tip-invalid object is remembered across tips: longer than the longest split-close assembly window
/// (`declared + 4 × count`, ≤ 108 DAA at the ruleset's 27 chunks).
pub(crate) const PALW_H1_INVALID_MEMORY_DAA_V1: u64 = 120;
/// The global rehearsal budget: a burst of this many units ...
pub(crate) const PALW_H1_BUDGET_BURST_UNITS_V1: u64 = 512;
/// ... refilled at this many units a second. One unit is one ordinary rehearsal (a state copy); honest H-1 traffic is a few carriers
/// a block.
pub(crate) const PALW_H1_BUDGET_UNITS_PER_SEC_V1: u64 = 64;
/// The units an objective offence weighs: it may recompute a whole 2M prompt root in the acceptance layer.
pub(crate) const PALW_H1_OFFENCE_UNITS_V1: u64 = 8;

/// Whether a refusal holds at every tip (the object's own fault: a relaying peer misbehaves) or can move with the next block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwH1RefusalClassV1 {
    Invalid,
    Timing,
}

/// The gate's refusal, classified. `memo == false` for a node-local resource refusal, which is never remembered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwH1RefusalV1 {
    pub class: PalwH1RefusalClassV1,
    pub why: String,
    pub memo: bool,
}

impl PalwH1RefusalV1 {
    pub(crate) fn timing(why: String) -> Self {
        Self { class: PalwH1RefusalClassV1::Timing, why, memo: true }
    }
    pub(crate) fn invalid(why: String) -> Self {
        Self { class: PalwH1RefusalClassV1::Invalid, why, memo: true }
    }
    /// The admission error: `PalwH1CarrierInvalid` for an every-tip refusal, `PalwH1CarrierRefused` (as before) otherwise.
    pub(crate) fn into_admission_error(self) -> kaspa_consensus_core::errors::tx::TxRuleError {
        use kaspa_consensus_core::errors::tx::TxRuleError;
        match self.class {
            PalwH1RefusalClassV1::Invalid => TxRuleError::PalwH1CarrierInvalid(self.why),
            PalwH1RefusalClassV1::Timing => TxRuleError::PalwH1CarrierRefused(self.why),
        }
    }
}

/// A map that keeps at most `cap` entries, dropping the oldest insertion first.
pub(crate) struct PalwBoundedMapV1<K, V> {
    map: HashMap<K, V>,
    order: VecDeque<K>,
    cap: usize,
}

impl<K: Eq + Hash + Clone, V> PalwBoundedMapV1<K, V> {
    pub(crate) fn new(cap: usize) -> Self {
        Self { map: HashMap::new(), order: VecDeque::new(), cap: cap.max(1) }
    }
    pub(crate) fn get(&self, key: &K) -> Option<&V> {
        self.map.get(key)
    }
    pub(crate) fn insert(&mut self, key: K, value: V) {
        if self.map.insert(key.clone(), value).is_none() {
            self.order.push_back(key);
            while self.order.len() > self.cap {
                if let Some(old) = self.order.pop_front() {
                    self.map.remove(&old);
                }
            }
        }
    }
    pub(crate) fn len(&self) -> usize {
        self.map.len()
    }
}

/// A token bucket over a monotonic clock: `burst` units, refilled at `per_sec`.
pub(crate) struct PalwTokenBucketV1 {
    burst: u64,
    per_sec: u64,
    /// Thousandths of a unit, so a slow refill is not lost to rounding.
    milli: u64,
    last: Option<Instant>,
}

impl PalwTokenBucketV1 {
    pub(crate) fn new(burst: u64, per_sec: u64) -> Self {
        Self { burst, per_sec, milli: burst.saturating_mul(1_000), last: None }
    }
    /// Take `units` at `now`; `false` (and nothing taken) when the bucket holds fewer.
    pub(crate) fn take(&mut self, units: u64, now: Instant) -> bool {
        if let Some(last) = self.last {
            let elapsed_ms = now.saturating_duration_since(last).as_millis().min(u64::MAX as u128) as u64;
            self.milli = self.milli.saturating_add(elapsed_ms.saturating_mul(self.per_sec)).min(self.burst.saturating_mul(1_000));
        }
        self.last = Some(now);
        let cost = units.saturating_mul(1_000);
        if self.milli < cost {
            return false;
        }
        self.milli -= cost;
        true
    }
}

/// The verdict memory's key: the sink the reader state stands at, the point's DAA and blue score, the object's digest.
type PalwH1VerdictKeyV1 = (BlockHash, u64, u64, Hash64);

/// The guard's state. Memory only; node-local; no verdict of any block reads it.
pub(crate) struct PalwH1GateGuardV1 {
    pub(crate) verdicts: PalwBoundedMapV1<PalwH1VerdictKeyV1, Option<PalwH1RefusalV1>>,
    pub(crate) refused_txids: PalwBoundedMapV1<kaspa_consensus_core::tx::TransactionId, (BlockHash, PalwH1RefusalV1)>,
    pub(crate) invalid_objects: PalwBoundedMapV1<Hash64, (u64, PalwH1RefusalV1)>,
    pub(crate) budget: PalwTokenBucketV1,
    /// Rehearsals actually run at admission (what the memories did not answer) — the number the storm tests bound.
    pub(crate) rehearsals: u64,
    /// Admissions refused over budget.
    pub(crate) throttled: u64,
}

impl Default for PalwH1GateGuardV1 {
    fn default() -> Self {
        Self {
            verdicts: PalwBoundedMapV1::new(PALW_H1_VERDICT_MEMO_CAP_V1),
            refused_txids: PalwBoundedMapV1::new(PALW_H1_REFUSED_TXIDS_CAP_V1),
            invalid_objects: PalwBoundedMapV1::new(PALW_H1_INVALID_OBJECTS_CAP_V1),
            budget: PalwTokenBucketV1::new(PALW_H1_BUDGET_BURST_UNITS_V1, PALW_H1_BUDGET_UNITS_PER_SEC_V1),
            rehearsals: 0,
            throttled: 0,
        }
    }
}

/// The object's identity for the memories: the borsh bytes under the court-close object domain (no temporary buffer).
pub(crate) fn palw_h1_object_digest_v1(object: &kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2) -> Option<Hash64> {
    kaspa_consensus_core::palw_state_v2::palw_court_close_object_digest_v1(object).ok()
}

impl VirtualStateProcessor {
    /// Whether any scheduled fence activates within one DAA of `daa_score` — there a refusal can be a flag-day race, so it is never
    /// classified [`PalwH1RefusalClassV1::Invalid`].
    pub(super) fn palw_h1_near_a_fence_v1(&self, daa_score: u64) -> bool {
        self.palw_h1_fence_heights_v1.iter().any(|at| at.abs_diff(daa_score) <= 1)
    }

    /// The units one admission rehearsal of `object` takes from the budget: a completing split close weighs one per stored chunk
    /// (the assembly it forces), an objective offence [`PALW_H1_OFFENCE_UNITS_V1`], anything else one.
    pub(super) fn palw_h1_rehearsal_units_v1(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        object: &kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2,
    ) -> u64 {
        use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2 as Obj;
        match object {
            Obj::CourtCloseChunk { session_id, side, .. }
                if kaspa_consensus_core::palw_state_v2::palw_court_close_completes_a_group_v1(state, object) =>
            {
                1 + state.court_close_group(session_id, *side).map_or(0, |group| group.count as u64)
            }
            Obj::ObjectiveEvidenceChunkV1 { .. }
                if kaspa_consensus_core::palw_state_v2::palw_court_close_completes_a_group_v1(state, object) =>
            {
                PALW_H1_OFFENCE_UNITS_V1
            }
            Obj::ObjectiveOffence { .. } => PALW_H1_OFFENCE_UNITS_V1,
            _ => 1,
        }
    }

    /// The gate on the canonical tip (`chain_point`, the reader state), through the verdict memory: the template's re-ask and the
    /// sweep. Exactly the gate's answer — the memory is keyed by every input the answer reads.
    pub(super) fn palw_h1_gate_memoised_v1(
        &self,
        object: &kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2,
        virtual_daa_score: u64,
        chain_point: BlockHash,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
    ) -> Option<PalwH1RefusalV1> {
        let state_params = self.palw_state_params_v2.as_ref()?;
        let point = self.palw_h1_gate_point_v1(object, virtual_daa_score, chain_point, state);
        let key = palw_h1_object_digest_v1(object).map(|digest| (chain_point, point.daa_score, point.blue_score, digest));
        if let Some(key) = &key
            && let Some(verdict) = self.palw_h1_guard.lock().verdicts.get(key)
        {
            return verdict.clone();
        }
        let verdict = self.palw_h1_carrier_refusal_typed_on(state, state_params, &point, object);
        if let Some(key) = key
            && verdict.as_ref().is_none_or(|refusal| refusal.memo)
        {
            self.palw_h1_guard.lock().verdicts.insert(key, verdict.clone());
        }
        verdict
    }

    /// **The gate at ADMISSION** (`validate_mempool_transaction_impl`): the memories, then the budget, then the gate itself, and the
    /// verdict remembered. `None` exactly where [`Self::palw_mempool_h1_carrier_refusal`] is `None` — except over budget, where the
    /// carrier is refused unjudged (`PalwH1CarrierThrottled`).
    pub(in crate::pipeline::virtual_processor) fn palw_mempool_h1_admission_refusal_v1(
        &self,
        tx: &Transaction,
        virtual_daa_score: u64,
    ) -> Option<kaspa_consensus_core::errors::tx::TxRuleError> {
        let object = self.palw_h1_gated_object_at(tx, virtual_daa_score)?;
        let state_params = self.palw_state_params_v2.as_ref()?;
        let (chain_point, state) = self.palw_v2_reader_state(state_params).ok().flatten()?;
        let txid = tx.id();
        let point = self.palw_h1_gate_point_v1(&object, virtual_daa_score, chain_point, &state);
        let digest = palw_h1_object_digest_v1(&object);
        let key = digest.map(|digest| (chain_point, point.daa_score, point.blue_score, digest));
        {
            let mut guard = self.palw_h1_guard.lock();
            // This txid, refused on this tip.
            if let Some((at, refusal)) = guard.refused_txids.get(&txid)
                && *at == chain_point
            {
                return Some(refusal.clone().into_admission_error());
            }
            // This object, refused as invalid at every tip, within its memory window.
            if let Some(digest) = digest
                && let Some((until, refusal)) = guard.invalid_objects.get(&digest)
                && virtual_daa_score <= *until
            {
                let refusal = refusal.clone();
                guard.refused_txids.insert(txid, (chain_point, refusal.clone()));
                return Some(refusal.into_admission_error());
            }
            // This object's verdict on this tip at this point.
            if let Some(key) = &key
                && let Some(verdict) = guard.verdicts.get(key)
            {
                let verdict = verdict.clone();
                if let Some(refusal) = &verdict {
                    guard.refused_txids.insert(txid, (chain_point, refusal.clone()));
                }
                return verdict.map(PalwH1RefusalV1::into_admission_error);
            }
            // The budget, before any rehearsal work.
            let units = self.palw_h1_rehearsal_units_v1(&state, &object);
            if !guard.budget.take(units, Instant::now()) {
                guard.throttled += 1;
                return Some(kaspa_consensus_core::errors::tx::TxRuleError::PalwH1CarrierThrottled(format!(
                    "{units} unit(s) wanted; the node rehearses at most {PALW_H1_BUDGET_UNITS_PER_SEC_V1} units a second \
                     (burst {PALW_H1_BUDGET_BURST_UNITS_V1})"
                )));
            }
            guard.rehearsals += 1;
        }
        let verdict = self.palw_h1_carrier_refusal_typed_on(&state, state_params, &point, &object);
        let mut guard = self.palw_h1_guard.lock();
        if let Some(key) = key
            && verdict.as_ref().is_none_or(|refusal| refusal.memo)
        {
            guard.verdicts.insert(key, verdict.clone());
        }
        if let Some(refusal) = &verdict
            && refusal.memo
        {
            guard.refused_txids.insert(txid, (chain_point, refusal.clone()));
            if refusal.class == PalwH1RefusalClassV1::Invalid
                && let Some(digest) = digest
            {
                guard.invalid_objects.insert(digest, (virtual_daa_score.saturating_add(PALW_H1_INVALID_MEMORY_DAA_V1), refusal.clone()));
            }
        }
        verdict.map(PalwH1RefusalV1::into_admission_error)
    }

    /// Test-only: **the gate exactly as it stood before the liveness hotfix** (int-12 `palw_h1_carrier_refusal_on`, verbatim) — the
    /// reference the decision-equivalence tests hold the guarded gate to.
    #[cfg(test)]
    pub(in crate::pipeline::virtual_processor) fn palw_h1_carrier_refusal_reference_v1(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        state_params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
        object: &kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2,
    ) -> Option<String> {
        if let Some(verdict) = self.palw_exec_origin_rehearsal_v1_on(state, state_params, point, object) {
            use kaspa_consensus_core::palw_producer_v2::PalwObjectRehearsalV1 as Verdict;
            return match verdict {
                Verdict::Accepted => None,
                Verdict::NotAccepted(why) => Some(why),
                Verdict::Refused(why) => Some(why.to_string()),
            };
        }
        if let Err(why) = self.palw_v2_validate_objects(state, state_params, point, std::slice::from_ref(object)) {
            return Some(why);
        }
        let refusal = kaspa_consensus_core::palw_state_v2::palw_v2_apply_one_object_retained_v1(
            state,
            state_params,
            point,
            object,
            self.palw_unavailable_abstains_at(point.daa_score),
            self.palw_capability_bound_at(point.daa_score),
            self.palw_uncertified_weightless_at(point.daa_score),
            self.palw_da_court_at(point.daa_score),
            &self.palw_transition_extras_for_objects(state, point, std::slice::from_ref(object)),
        )
        .err()?;
        if matches!(refusal, kaspa_consensus_core::palw_state_v2::PalwStateV2Error::DaOpeningRefused { .. })
            && self.palw_legacy_public_filer_at(point.daa_score)
            && let kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::MaterialDisclosedV2 {
                claim,
                unit: kaspa_consensus_core::palw_da_rcore_v1::PalwDaUnitV1::Event { row: 0, tile: 0 },
                answer: kaspa_consensus_core::palw_da_rcore_v1::PalwDaAnswerV1::Event(event),
                ..
            } = object
            && let Some(target) = kaspa_consensus_core::palw_offence_attribution_v1::palw_offence_target_v1(state, claim)
            && let Ok(proof) = kaspa_consensus_core::palw_replay_refute_v1::palw_replay_executor_refuted_object_v1(
                *claim,
                target.executor_bond,
                kaspa_consensus_core::palw_offence_v1::PalwPanelContradictionV1::IdentityMismatch { binding: event.binding().clone() },
                None,
            )
            && matches!(
                self.palw_object_rehearsal_v1_on(state, state_params, point, &proof.object),
                kaspa_consensus_core::palw_producer_v2::PalwObjectRehearsalV1::Accepted
            )
        {
            // The DA arm already checked the live demand and discloser's liability; the
            // acceptance layer checked its signature/size. The second rehearsal proves the
            // claim's authenticated identity fault. Carriage changes no DA answered bit and
            // applies no conviction: the outside verifier still files its own proof.
            return None;
        }
        Some(refusal.to_string())
    }


    /// Test-only: the guard's counters `(rehearsals run at admission, admissions throttled)`.
    #[cfg(test)]
    pub(crate) fn palw_h1_guard_counters_for_tests(&self) -> (u64, u64) {
        let guard = self.palw_h1_guard.lock();
        (guard.rehearsals, guard.throttled)
    }

    /// Test-only: replace the budget (`burst`, `per_sec`).
    #[cfg(test)]
    pub(crate) fn palw_h1_set_budget_for_tests(&self, burst: u64, per_sec: u64) {
        self.palw_h1_guard.lock().budget = PalwTokenBucketV1::new(burst, per_sec);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn the_bounded_map_keeps_the_newest_cap_entries() {
        let mut map = PalwBoundedMapV1::new(3);
        for i in 0..10u64 {
            map.insert(i, i);
        }
        assert_eq!(map.len(), 3);
        assert!(map.get(&6).is_none() && map.get(&7).is_some() && map.get(&9).is_some());
        map.insert(9, 99);
        assert_eq!((map.len(), map.get(&9)), (3, Some(&99)), "a re-insert replaces in place");
    }

    #[test]
    fn the_token_bucket_spends_its_burst_then_refills_at_its_rate() {
        let t0 = Instant::now();
        let mut bucket = PalwTokenBucketV1::new(4, 2);
        assert!((0..4).all(|_| bucket.take(1, t0)), "the burst");
        assert!(!bucket.take(1, t0), "empty");
        assert!(!bucket.take(2, t0 + Duration::from_millis(500)), "half a second refills one unit, not two");
        assert!(bucket.take(1, t0 + Duration::from_millis(500)));
        assert!(bucket.take(4, t0 + Duration::from_secs(60)), "a long pause refills to the burst ...");
        assert!(!bucket.take(1, t0 + Duration::from_secs(60)), "... and no further");
        assert!(!bucket.take(5, t0 + Duration::from_secs(600)), "a weight above the burst never passes");
    }
}
