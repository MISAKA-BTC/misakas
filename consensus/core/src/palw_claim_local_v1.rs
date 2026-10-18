//! RFC-0023 stage 2: claim dependencies and a material-bearing liability checkpoint.
//!
//! This module supplies the deterministic checkpoint checks. Runtime admission must derive the
//! records from committed inputs and reservations; a producer's list is never the authority.
//! Integration status: dependency/checkpoint foundations and the dormant fence only. Runtime
//! admission, reward/credit folding, native lifecycle, pruning and IBD are not connected yet.
//! Hashes below authenticate bytes only. Certification remains the selected validators' approval
//! under Gate C's stated honest-fraction premise; no computation-validity proof is introduced.
use crate::palw_state_v2::PalwBlockContextV2;
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_hashes::{Hash64, ZERO_HASH64};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub const PALW_CLAIM_LOCAL_TABLE_V1: u8 = 144;

/// Actual presented inputs, retained without declaring them valid. Only the
/// fold's resulting claim rows and journal establish what was admitted.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwClaimLocalInputV1 {
    Attempt {
        block: Hash64,
        execution_key: Hash64,
        envelope: Box<crate::palw_attempt_v2::PalwAttemptEnvelopeV2>,
    },
    ReceiptSpend {
        block: Hash64,
        spend: Box<crate::palw_freeprompt_v3::PalwReceiptSpendUnsignedV3>,
    },
    History {
        claim: Hash64,
        pledge: Box<crate::palw_history_custody_v1::PalwHistoryCustodyPledgeV1>,
    },
    PublicBinding {
        claim: Hash64,
        header: Box<crate::palw_public_binding_v1::PalwPublicExecutionBindingHeaderV1>,
    },
    CustodyDelegation {
        claim: Hash64,
        delegation: Box<crate::palw_history_custody_v1::PalwProducerCustodyDelegationV1>,
    },
    TirOriginal {
        claim: Hash64,
        envelope: Box<crate::palw_tir_public_fp_envelope_v1::PalwTirPublicFpEnvelopeV1>,
    },
    /// The legacy object's three native-only fields are skipped by its wire
    /// Borsh codec. Archive them separately; never change that released codec.
    FreePromptNative {
        claim: Hash64,
        job_pin: Hash64,
        public_job: Option<Box<crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3>>,
        eval: Option<Box<(crate::palw_improve_eval_v1::PalwEvalJobV1, crate::palw_improve_eval_v1::PalwEvalClaimTailV1)>>,
    },
}

/// Values in force at this fold point, retained independently of their IDs.
/// This is historical context, not authority to invent an older claim's policy.
/// Native per-job policies and all route-specific covenants still belong in
/// their original admitted material; these fields do not replace them.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwClaimLocalPolicyContextV1 {
    pub clocks: PalwClaimLocalClockPolicyV1,
    pub detection: Vec<crate::palw_detection_policy_v1::PalwDetectionPolicyV1>,
    pub certification: Option<crate::palw_security_certification_v1::PalwSecurityCertificationPolicyV1>,
    pub court_capacity: Option<crate::palw_court_capacity_v1::PalwCourtCapacityPolicyV1>,
}

/// Base windows at the fold point; per-class / already-open session deadlines
/// are retained in the two state carriages, never reconstructed from these.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwClaimLocalClockPolicyV1 {
    pub bind: u64,
    pub receipt: u64,
    pub challenge: u64,
    pub court: u64,
    pub turn: u64,
    pub retirement: u64,
    pub epoch: u64,
}

impl PalwClaimLocalPolicyContextV1 {
    /// Borrow the effective list so its allocation can be reserved before copying.
    pub(crate) fn detection_at_v1(
        params: &crate::palw_state_v2::PalwStateParamsV2,
        daa: u64,
    ) -> &[crate::palw_detection_policy_v1::PalwDetectionPolicyV1] {
        params
            .detection_policy_v1()
            .filter(|f| f.activation != crate::config::params::ForkActivation::never() && f.activation.is_active(daa))
            .map_or(&[], |f| f.policies.as_slice())
    }

    /// The caller reserves the borrowed detection list before constructing this copy.
    pub(crate) fn from_rules_v1(params: &crate::palw_state_v2::PalwStateParamsV2, daa: u64) -> Self {
        Self {
            clocks: PalwClaimLocalClockPolicyV1 {
                bind: params.window_bind(),
                receipt: params.window_receipt(),
                challenge: params.window_challenge_at(daa),
                court: params.window_court(),
                turn: params.turn_deadline_daa(),
                retirement: params.claim_retirement_daa(),
                epoch: params.epoch_length(),
            },
            detection: Self::detection_at_v1(params, daa).to_vec(),
            certification: params.rfc0018_v1().filter(|f| f.gate_c_at(daa)).map(|f| f.certification),
            court_capacity: params.court_capacity_at_v1(daa),
        }
    }
}

/// Fold-produced historical context for the claims actually changed at a DAG
/// point. The two carriages omit this namespace: a frame never contains frames.
/// These are original state/identity witnesses, not declarations of correctness.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwClaimLiabilityFrameV1 {
    pub point: PalwBlockContextV2,
    pub network_identity: [u8; 32],
    pub fired_forks: [u8; 32],
    pub policy: PalwClaimLocalPolicyContextV1,
    pub claims: BTreeSet<Hash64>,
    pub prior_state: Vec<u8>,
    pub current_state: Vec<u8>,
    pub accepted_objects: Vec<crate::palw_state_v2::PalwConsensusObjectV2>,
    pub origin_headers: BTreeMap<Hash64, Vec<u8>>,
    pub inputs: Vec<PalwClaimLocalInputV1>,
}

pub fn palw_claim_liability_frame_key_v1(block: Hash64) -> [u8; 65] {
    let mut key = [0u8; 65];
    key[0] = b'F';
    key[1..].copy_from_slice(block.as_byte_slice());
    key
}

impl PalwClaimLiabilityFrameV1 {
    /// Structural/context consistency is necessary, not approval of the claims
    /// or authority for a sender's invented historical state. The frame must
    /// also be a row of the accepted checkpoint's committed native state.
    pub fn validate_context_v1(&self) -> Result<(), PalwClaimCheckpointErrorV1> {
        self.with_context_carriages_v1(|_, _| Ok(()))
    }

    /// Keep the preflight reservation through the whole borrowed inspection.
    /// A callback cannot return a reference into a discarded carriage.
    pub(crate) fn with_context_carriages_v1<T>(
        &self,
        inspect: impl FnOnce(
            &crate::palw_state_v2::PalwStateCarriageV2,
            &crate::palw_state_v2::PalwStateCarriageV2,
        ) -> Result<T, PalwClaimCheckpointErrorV1>,
    ) -> Result<T, PalwClaimCheckpointErrorV1> {
        use crate::palw_native_resource_v1::{preflight_native_carriage_v1, reserve_native_requests_v1};
        use crate::palw_state_v2::PalwStateCarriageV2;
        if self.claims.is_empty()
            || self.claims.contains(&ZERO_HASH64)
            || self.network_identity == [0; 32]
            || self.fired_forks == [0; 32]
        {
            return Err("liability frame has no claim or original rule identity".into());
        }
        let mut demands = 0u64;
        for bytes in [&self.prior_state, &self.current_state] {
            let scan = preflight_native_carriage_v1(bytes.as_slice(), bytes.len() as u64)
                .map_err(|e| claim_local_checkpoint_io_v1(e, "malformed original liability carriage"))?;
            demands = demands
                .checked_add(
                    scan.memory_requests_v1().map_err(|e| claim_local_checkpoint_io_v1(e, "original carriage demand overflow"))?,
                )
                .ok_or_else(|| {
                    PalwClaimCheckpointErrorV1::Resource(crate::palw_native_resource_v1::NativeCoverageV1::overflow::<Self>(
                        "original context demand",
                    ))
                })?;
        }
        let _lease = reserve_native_requests_v1(demands).map_err(PalwClaimCheckpointErrorV1::Resource)?;
        let prior: PalwStateCarriageV2 = borsh::from_slice(&self.prior_state)
            .map_err(|_| PalwClaimCheckpointErrorV1::Semantic("original prior state does not decode"))?;
        let current: PalwStateCarriageV2 = borsh::from_slice(&self.current_state)
            .map_err(|_| PalwClaimCheckpointErrorV1::Semantic("original current state does not decode"))?;
        for (state, bytes) in [(&prior, &self.prior_state), (&current, &self.current_state)] {
            palw_check_canonical_liability_bytes_v1(state, bytes)
                .map_err(|_| PalwClaimCheckpointErrorV1::Semantic("noncanonical original state"))?;
            if state.kernel_route.as_ref().is_some_and(|route| route.aux.keys().any(|(table, _)| *table == PALW_CLAIM_LOCAL_TABLE_V1))
            {
                return Err("liability frame contains its own namespace".into());
            }
        }
        if current.last_point != Some(self.point)
            || prior.last_point.is_some_and(|p| p.blue_score >= self.point.blue_score || p.daa_score > self.point.daa_score)
        {
            return Err("liability frame changed its original DAG point".into());
        }
        inspect(&prior, &current)
    }
}

/// One chain claim's birth in an executed state, distinguished from later
/// phase/policy changes and from its RED carrying header. This is metadata
/// provenance, not computation correctness or a complete-material capability.
/// Only the state's authenticated frame reader constructs it.
pub struct PalwClaimLocalAdmissionContextV1 {
    pub(crate) id: Hash64,
    pub(crate) frame: crate::palw_native_resource_v1::NativeRetainedV1<PalwClaimLiabilityFrameV1>,
    pub(crate) claim: crate::palw_native_resource_v1::NativeRetainedV1<crate::palw_state_v2::PalwClaimStateV2>,
}
impl PalwClaimLocalAdmissionContextV1 {
    pub fn claim_id(&self) -> Hash64 {
        self.id
    }
    pub fn accepting_point(&self) -> PalwBlockContextV2 {
        self.frame.point
    }
    pub fn original_claim(&self) -> &crate::palw_state_v2::PalwClaimStateV2 {
        &self.claim
    }
    pub fn frame(&self) -> &PalwClaimLiabilityFrameV1 {
        &self.frame
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwClaimLocalRecordV1 {
    pub claim: Hash64,
    pub accepted_daa: u64,
    pub accepted_blue_score: u64,
    /// The shared-DAG block that accepted this claim, including when its carrying block was RED.
    pub accepting_block: Hash64,
    /// Only an attempt produces a frontier anchor; FP receipt work alone does not.
    pub anchors: bool,
    /// Actual consumed outputs; populated by admission, never inferred from model identity.
    pub inputs: BTreeSet<Hash64>,
    pub execution_root: Hash64,
    /// Reservation identities, not bond identities: two reservations on one bond remain separate.
    pub reservations: BTreeSet<Hash64>,
    pub final_claim: bool,
    pub certified: bool,
    pub voided: bool,
    /// True only after all applicable prosecution, DA and court obligations have closed.
    pub liability_closed: bool,
    /// Every byte object needed to pursue this liability, including its pinned policy,
    /// registry/randomness evidence and open court state. The admission-derived manifest is exact.
    pub material: BTreeSet<Hash64>,
}

pub fn palw_liability_material_id_v1(bytes: &[u8]) -> Hash64 {
    let hash = blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/liability-material/v1").hash(bytes);
    let mut out = [0u8; 64];
    out.copy_from_slice(hash.as_bytes());
    Hash64::from_bytes(out)
}

/// A checkpoint includes the actual material. A root or a URL cannot stand in for missing bytes.
/// The exact liability state contains collateral reservations, detection policies, registration /
/// randomness evidence and court state as canonically derived by the fold at this point.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwClaimLocalCheckpointV1 {
    pub point: PalwBlockContextV2,
    pub records: BTreeMap<Hash64, PalwClaimLocalRecordV1>,
    pub liability_state: Vec<u8>,
    pub material: BTreeMap<Hash64, Vec<u8>>,
}

#[derive(Debug, thiserror::Error)]
pub enum PalwClaimCheckpointErrorV1 {
    #[error("{0}")]
    Semantic(&'static str),
    #[error("checkpoint validation needs local native capacity: {0}")]
    Resource(crate::palw_native_resource_v1::NativeCoverageV1),
}
// Local scanner limitations must never become a consensus verdict against
// valid bytes. Keep malformed/truncated bytes distinct from capacity refusal.
fn claim_local_decode_io_v1<T>(error: std::io::Error) -> crate::palw_native_resource_v1::NativeDecodeErrorV1<std::io::Error> {
    use crate::palw_native_resource_v1::{NativeCoverageV1, NativeDecodeErrorV1};
    use std::io::ErrorKind;
    match error.kind() {
        ErrorKind::OutOfMemory => NativeDecodeErrorV1::Resource(NativeCoverageV1::overflow::<T>("claim-local scan")),
        ErrorKind::Unsupported | ErrorKind::TimedOut => {
            NativeDecodeErrorV1::Resource(NativeCoverageV1::unsupported::<T>("claim-local scan", &error.to_string()))
        }
        _ => NativeDecodeErrorV1::Decode(error),
    }
}
fn claim_local_checkpoint_io_v1(error: std::io::Error, malformed: &'static str) -> PalwClaimCheckpointErrorV1 {
    match claim_local_decode_io_v1::<PalwClaimLiabilityFrameV1>(error) {
        crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(why) => PalwClaimCheckpointErrorV1::Resource(why),
        crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(_) => PalwClaimCheckpointErrorV1::Semantic(malformed),
    }
}
impl From<&'static str> for PalwClaimCheckpointErrorV1 {
    fn from(why: &'static str) -> Self {
        Self::Semantic(why)
    }
}

impl PalwClaimLocalCheckpointV1 {
    /// Both expected arguments come from committed state, not the checkpoint sender. Comparing the
    /// complete material manifest prevents a sender replacing a complete archive with one spot check.
    pub fn validate(
        &self,
        point: &PalwBlockContextV2,
        expected: &BTreeMap<Hash64, PalwClaimLocalRecordV1>,
        expected_liability_state: &[u8],
    ) -> Result<PalwValidatedClaimCheckpointV1<'_>, PalwClaimCheckpointErrorV1> {
        if self.point != *point || self.records != *expected || self.liability_state != expected_liability_state {
            return Err("checkpoint differs from the fold's liability state".into());
        }
        use crate::palw_native_resource_v1::{NativeHeapV1, reserve_native_requests_v1};
        // Record envelopes dominate the vertex maps/queue; input/material set
        // envelopes dominate dedup sets and twice-grown consumer vectors. Four
        // copies cover both validation and the largest subsequent graph read.
        // Retain the ticket for the validated reader's lifetime.
        let bound = self.records.native_heap_v1("claim checkpoint graph").scale::<Self>(4, "claim checkpoint graph workspace");
        let requests = bound.bytes().ok_or(PalwClaimCheckpointErrorV1::Resource(bound))?;
        let workspace = reserve_native_requests_v1(requests).map_err(PalwClaimCheckpointErrorV1::Resource)?;
        let mut required = BTreeSet::new();
        for (id, record) in &self.records {
            if *id != record.claim || record.accepted_daa > point.daa_score || record.accepted_blue_score > point.blue_score {
                return Err("checkpoint claim identity or acceptance point differs".into());
            }
            if record.inputs.contains(id)
                || record.inputs.iter().any(|input| {
                    self.records
                        .get(input)
                        .is_some_and(|r| r.accepted_daa > record.accepted_daa || r.accepted_blue_score > record.accepted_blue_score)
                })
            {
                return Err("checkpoint input was not available at admission".into());
            }
            if (record.voided && (record.final_claim || record.certified))
                || (record.certified && !record.final_claim)
                || (record.liability_closed && !record.final_claim && !record.voided)
            {
                return Err("checkpoint phase, certification and liability flags disagree".into());
            }
            if record.inputs.iter().any(|input| !self.records.contains_key(input)) {
                return Err("checkpoint lost a consumed claim".into());
            }
            if !record.liability_closed {
                if record.material.is_empty() {
                    return Err("open liability has no material manifest".into());
                }
                required.extend(record.material.iter().copied());
            }
        }
        // Actual consumed outputs form a causal DAG. Shared execution / reservation groups are
        // handled separately below and may be mutual; they must not mask a cycle in input provenance.
        let mut pending: BTreeMap<Hash64, usize> = self.records.iter().map(|(id, r)| (*id, r.inputs.len())).collect();
        let mut consumers: BTreeMap<Hash64, Vec<Hash64>> = BTreeMap::new();
        for (id, record) in &self.records {
            for input in &record.inputs {
                consumers.entry(*input).or_default().push(*id);
            }
        }
        let mut ready: VecDeque<Hash64> = pending.iter().filter(|(_, n)| **n == 0).map(|(id, _)| *id).collect();
        let mut visited = 0;
        while let Some(id) = ready.pop_front() {
            visited += 1;
            for consumer in consumers.get(&id).into_iter().flatten() {
                let n = pending.get_mut(consumer).expect("consumer is a checkpoint record");
                *n -= 1;
                if *n == 0 {
                    ready.push_back(*consumer);
                }
            }
        }
        if visited != self.records.len() {
            return Err("checkpoint input provenance contains a cycle".into());
        }
        if !required.is_empty() && self.liability_state.is_empty() {
            return Err("open liabilities lost their policy, registry, randomness or court state".into());
        }
        if required != self.material.keys().copied().collect() {
            return Err("checkpoint is missing material or contains unbound material".into());
        }
        if self.material.iter().any(|(id, bytes)| palw_liability_material_id_v1(bytes) != *id) {
            return Err("checkpoint material bytes differ from their identity".into());
        }
        Ok(PalwValidatedClaimCheckpointV1 { checkpoint: self, _workspace: workspace })
    }

    /// The dependency closure includes shared execution and reservation identities. Model IDs
    /// deliberately do not occur in this schema. Missing input provenance fails closed.
    fn dependencies(&self, claim: Hash64) -> Result<BTreeSet<Hash64>, &'static str> {
        if !self.records.contains_key(&claim) {
            return Err("missing dependency record");
        }
        let mut seen = BTreeSet::from([claim]);
        let mut queue = VecDeque::from([claim]);
        while let Some(id) = queue.pop_front() {
            let record = self.records.get(&id).ok_or("missing dependency record")?;
            for input in &record.inputs {
                if seen.insert(*input) {
                    queue.push_back(*input);
                }
            }
            for (other_id, other) in &self.records {
                // A later reuse consumes the earlier identity. It cannot
                // retroactively reopen an earlier, irrevocable checkpoint.
                // Claims accepted together share liability mutually.
                if other.accepted_blue_score <= record.accepted_blue_score
                    && other.accepted_daa <= record.accepted_daa
                    && ((record.execution_root != ZERO_HASH64 && record.execution_root == other.execution_root)
                        || !record.reservations.is_disjoint(&other.reservations))
                {
                    if seen.insert(*other_id) {
                        queue.push_back(*other_id);
                    }
                }
            }
        }
        Ok(seen)
    }

    /// Reversible credit / reward eligibility. Liability expiry alone never certifies a claim.
    fn credit_eligible(&self, claim: Hash64) -> bool {
        self.dependencies(claim).is_ok_and(|ids| {
            ids.iter().all(|id| {
                let r = &self.records[id];
                r.final_claim && r.certified && !r.voided
            })
        })
    }

    /// Irreversible checkpoints use only credit whose complete dependency closure is past every
    /// liability. An unresolved claim remains in the checkpoint and supplies no such credit.
    fn irreversible_eligible(&self, claim: Hash64) -> bool {
        self.credit_eligible(claim) && self.dependencies(claim).is_ok_and(|ids| ids.iter().all(|id| self.records[id].liability_closed))
    }

    /// Revocation is directed: users of A are affected by A; A is not affected by a fault in a
    /// consumer. Shared execution roots and reservations form genuine shared liability groups.
    fn affected_by(&self, convicted: Hash64) -> Result<BTreeSet<Hash64>, &'static str> {
        if !self.records.contains_key(&convicted) {
            return Err("convicted claim is absent from the checkpoint");
        }
        let mut affected = BTreeSet::new();
        for id in self.records.keys() {
            if self.dependencies(*id)?.contains(&convicted) {
                affected.insert(*id);
            }
        }
        Ok(affected)
    }
}

/// Preflight untrusted checkpoint bytes before Borsh allocates. The returned
/// owner keeps its allocation ticket; callers must still use `validate` with
/// the executed state's records and liability projection before reading it.
pub fn palw_decode_claim_checkpoint_v1(
    bytes: &[u8],
) -> Result<
    crate::palw_native_resource_v1::NativeRetainedV1<PalwClaimLocalCheckpointV1>,
    crate::palw_native_resource_v1::NativeDecodeErrorV1<std::io::Error>,
> {
    decode_claim_local_value_v1(bytes)
}

pub fn palw_decode_claim_liability_frame_v1(
    bytes: &[u8],
) -> Result<
    crate::palw_native_resource_v1::NativeRetainedV1<PalwClaimLiabilityFrameV1>,
    crate::palw_native_resource_v1::NativeDecodeErrorV1<std::io::Error>,
> {
    decode_claim_local_value_v1(bytes)
}

fn decode_claim_local_value_v1<T: BorshDeserialize + BorshSerialize + crate::palw_native_resource_v1::NativeWireScanV1>(
    bytes: &[u8],
) -> Result<crate::palw_native_resource_v1::NativeRetainedV1<T>, crate::palw_native_resource_v1::NativeDecodeErrorV1<std::io::Error>> {
    use crate::palw_native_resource_v1::{
        NativeDecodeErrorV1 as D, NativeRetainedV1, preflight_native_value_v1, reserve_native_requests_v1,
    };
    // Unconditional shape validation also protects configurations without a
    // local quota. A huge nested count in a short frame never reaches Borsh.
    let shape = preflight_native_value_v1::<T>(bytes, bytes.len() as u64).map_err(claim_local_decode_io_v1::<T>)?;
    let lease = reserve_native_requests_v1(shape.logical_requests_v1()).map_err(D::Resource)?;
    let value = borsh::from_slice(bytes).map_err(D::Decode)?;
    palw_check_canonical_liability_bytes_v1(&value, bytes).map_err(D::Decode)?;
    Ok(NativeRetainedV1::from_owner_reserved_v1(value, lease))
}

/// Reject duplicate/reordered map and set encodings without allocating a
/// second encoded payload. Borsh's permissive map reader may otherwise erase
/// duplicates before the executed-state comparison can observe them.
pub(crate) fn palw_check_canonical_liability_bytes_v1<T: BorshSerialize>(value: &T, bytes: &[u8]) -> std::io::Result<()> {
    struct Exact<'a> {
        bytes: &'a [u8],
        at: usize,
    }
    impl std::io::Write for Exact<'_> {
        fn write(&mut self, chunk: &[u8]) -> std::io::Result<usize> {
            let end = self.at.checked_add(chunk.len()).ok_or_else(|| std::io::Error::other("canonical extent overflow"))?;
            if self.bytes.get(self.at..end) != Some(chunk) {
                return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "noncanonical liability encoding"));
            }
            self.at = end;
            Ok(chunk.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = Exact { bytes, at: 0 };
    borsh::to_writer(&mut writer, value)?;
    if writer.at != bytes.len() {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "noncanonical liability extent"));
    }
    Ok(())
}

#[path = "palw_native_resource_v1/private/palw_claim_local_v1.rs"]
mod native_resource_inventory_v1;
pub(crate) fn palw_claim_local_frame_scan_active_v1() -> bool {
    native_resource_inventory_v1::frame_scan_active_v1()
}

/// Read capabilities exist only after exact state and byte validation. Runtime code cannot rank
/// forks or settle effects directly from a peer-supplied checkpoint's self-declared flags.
pub struct PalwValidatedClaimCheckpointV1<'a> {
    checkpoint: &'a PalwClaimLocalCheckpointV1,
    _workspace: Option<crate::palw_native_resource_v1::NativeDemandLeaseV1>,
}
impl PalwValidatedClaimCheckpointV1<'_> {
    pub fn credit_eligible(&self, claim: Hash64) -> bool {
        self.checkpoint.credit_eligible(claim)
    }
    pub fn irreversible_eligible(&self, claim: Hash64) -> bool {
        self.checkpoint.irreversible_eligible(claim)
    }
    pub fn dependencies(&self, claim: Hash64) -> Result<BTreeSet<Hash64>, &'static str> {
        self.checkpoint.dependencies(claim)
    }
    pub fn affected_by(&self, claim: Hash64) -> Result<BTreeSet<Hash64>, &'static str> {
        self.checkpoint.affected_by(claim)
    }
    /// Reversible frontier for fork ranking, separate from the irreversible checkpoint support.
    pub fn frontier(&self) -> Option<&PalwClaimLocalRecordV1> {
        self.checkpoint
            .records
            .values()
            .filter(|r| r.anchors && self.credit_eligible(r.claim))
            .max_by_key(|r| (r.accepted_blue_score, r.claim))
    }
    pub fn irreversible_frontier(&self) -> Option<&PalwClaimLocalRecordV1> {
        self.checkpoint
            .records
            .values()
            .filter(|r| r.anchors && self.irreversible_eligible(r.claim))
            .max_by_key(|r| (r.accepted_blue_score, r.claim))
    }
    /// Settlement of recorded effects does not assert that every recorded claim is correct. The
    /// unresolved set and its retrievable material remain bound, and only irrevocable credit backs it.
    pub fn settled(&self, effect_daa: u64) -> bool {
        self.irreversible_frontier().is_some_and(|r| r.accepted_daa >= effect_daa)
    }
    pub fn lifecycle_preserved_through(&self, blue: u64) -> bool {
        if blue > self.checkpoint.point.blue_score {
            return false;
        }
        self.checkpoint
            .records
            .values()
            .filter(|r| r.accepted_blue_score <= blue && !r.liability_closed)
            .all(|r| !r.material.is_empty() && r.material.iter().all(|m| self.checkpoint.material.contains_key(m)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn id(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }
    fn fixture() -> PalwClaimLocalCheckpointV1 {
        let data = b"public replay data, pinned policy, draw and court state".to_vec();
        let material = palw_liability_material_id_v1(&data);
        let mut records = BTreeMap::new();
        for n in 1..=5 {
            records.insert(
                id(n),
                PalwClaimLocalRecordV1 {
                    claim: id(n),
                    accepted_daa: n,
                    accepted_blue_score: n,
                    accepting_block: id(300 + n),
                    anchors: true,
                    inputs: BTreeSet::new(),
                    execution_root: id(100 + n),
                    reservations: BTreeSet::from([id(200 + n)]),
                    final_claim: n != 1,
                    certified: n != 1,
                    voided: false,
                    liability_closed: n == 2,
                    material: BTreeSet::from([material]),
                },
            );
        }
        records.get_mut(&id(3)).unwrap().inputs.insert(id(1));
        records.get_mut(&id(4)).unwrap().execution_root = id(101);
        records.get_mut(&id(5)).unwrap().reservations = BTreeSet::from([id(201)]);
        PalwClaimLocalCheckpointV1 {
            point: PalwBlockContextV2 { block: id(400), daa_score: 10, blue_score: 10, subsidy: 0 },
            records,
            liability_state: vec![1, 2, 3],
            material: BTreeMap::from([(material, data)]),
        }
    }
    #[test]
    fn rfc23s2_independence_shared_execution_and_actual_reservations() {
        let c = fixture();
        assert!(c.credit_eligible(id(2)));
        assert!(c.irreversible_eligible(id(2)));
        let checked = c.validate(&c.point, &c.records, &c.liability_state).unwrap();
        assert!(checked.settled(2));
        assert!(!checked.settled(3));
        assert!(checked.lifecycle_preserved_through(5));
        assert!(!checked.lifecycle_preserved_through(11), "a checkpoint cannot cover a future effect");
        assert_eq!(checked.irreversible_frontier().unwrap().claim, id(2));
        for n in [1, 3, 4, 5] {
            assert!(!c.credit_eligible(id(n)));
        }
        assert_eq!(c.affected_by(id(1)).unwrap(), BTreeSet::from([id(1), id(3), id(4), id(5)]));
        assert_eq!(c.affected_by(id(3)).unwrap(), BTreeSet::from([id(3)]));
    }
    #[test]
    fn rfc23s2_checkpoint_requires_the_exact_state_and_actual_bound_material() {
        let c = fixture();
        c.validate(&c.point, &c.records, &c.liability_state).unwrap();
        let mut bad = c.clone();
        bad.material.clear();
        assert!(bad.validate(&c.point, &c.records, &c.liability_state).is_err());
        let mut bad = c.clone();
        bad.material.values_mut().next().unwrap()[0] ^= 1;
        assert!(bad.validate(&c.point, &c.records, &c.liability_state).is_err());
        let mut bad = c.clone();
        bad.records.remove(&id(1));
        assert!(bad.validate(&c.point, &c.records, &c.liability_state).is_err());
        assert!(c.validate(&PalwBlockContextV2 { daa_score: 11, ..c.point }, &c.records, &c.liability_state).is_err());
        let mut cycle = c.clone();
        cycle.records.get_mut(&id(1)).unwrap().inputs.insert(id(3));
        cycle.records.get_mut(&id(3)).unwrap().accepted_daa = 1;
        cycle.records.get_mut(&id(3)).unwrap().accepted_blue_score = 1;
        assert!(cycle.validate(&cycle.point, &cycle.records, &cycle.liability_state).is_err());
    }
    #[test]
    fn rfc23s2_heartbeat_and_free_prompt_alone_cannot_support_a_checkpoint() {
        let mut c = fixture();
        c.records.clear();
        c.material.clear();
        let checked = c.validate(&c.point, &c.records, &c.liability_state).unwrap();
        assert!(checked.frontier().is_none());
        assert!(!checked.settled(0));
        let mut c = fixture();
        for r in c.records.values_mut() {
            r.anchors = false;
        }
        let checked = c.validate(&c.point, &c.records, &c.liability_state).unwrap();
        assert!(checked.credit_eligible(id(2)), "FP may have certified spend weight");
        assert!(checked.frontier().is_none());
        assert!(!checked.settled(2), "FP never becomes an attempt anchor");
    }

    #[test]
    fn rfc23s2_checkpoint_codec_and_late_conviction_keep_independent_support() {
        let c = fixture();
        let bytes = borsh::to_vec(&c).unwrap();
        let mut restored: PalwClaimLocalCheckpointV1 = borsh::from_slice(&bytes).unwrap();
        restored.validate(&c.point, &c.records, &c.liability_state).unwrap();
        restored.records.get_mut(&id(1)).unwrap().voided = true;
        assert!(restored.irreversible_eligible(id(2)));
        assert_eq!(restored.affected_by(id(1)).unwrap(), c.affected_by(id(1)).unwrap());
        let reverted: PalwClaimLocalCheckpointV1 = borsh::from_slice(&bytes).unwrap();
        assert_eq!(reverted, c);
    }

    #[test]
    fn rfc23s2_future_reuse_cannot_reopen_irrevocable_credit() {
        let mut c = fixture();
        let mut later = c.records[&id(2)].clone();
        later.claim = id(6);
        later.accepted_daa = 6;
        later.accepted_blue_score = 6;
        later.accepting_block = id(306);
        later.final_claim = false;
        later.certified = false;
        later.liability_closed = false;
        c.records.insert(later.claim, later);
        let checked = c.validate(&c.point, &c.records, &c.liability_state).unwrap();
        assert!(checked.irreversible_eligible(id(2)));
        assert!(!checked.credit_eligible(id(6)));
        assert_eq!(checked.affected_by(id(6)).unwrap(), BTreeSet::from([id(6)]));
        assert_eq!(checked.affected_by(id(2)).unwrap(), BTreeSet::from([id(2), id(6)]));
        assert!(checked.settled(2));
    }

    #[test]
    fn rfc23s2_checkpoint_native_decode_covers_real_heap_and_rejects_hostile_counts() {
        use crate::palw_native_resource_v1::{NativeHeapV1, preflight_native_value_v1};
        let c = fixture();
        let bytes = borsh::to_vec(&c).unwrap();
        let shape = preflight_native_value_v1::<PalwClaimLocalCheckpointV1>(bytes.as_slice(), bytes.len() as u64).unwrap();
        assert!(
            shape.logical_requests_v1()
                >= c.native_heap_v1("fixture").bytes().unwrap() + std::mem::size_of::<PalwClaimLocalCheckpointV1>() as u64
        );
        let restored = palw_decode_claim_checkpoint_v1(&bytes).unwrap();
        assert_eq!(*restored, c);
        let checked = restored.validate(&c.point, &c.records, &c.liability_state).unwrap();
        assert!(checked.settled(2));
        assert!(checked.dependencies(id(999)).is_err());
        let mut hostile = borsh::to_vec(&c.point).unwrap();
        hostile.extend(u32::MAX.to_le_bytes());
        assert!(palw_decode_claim_checkpoint_v1(&hostile).is_err());
        assert!(palw_decode_claim_checkpoint_v1(&bytes[..bytes.len() - 1]).is_err());
        let mut trailing = bytes;
        trailing.push(0);
        assert!(palw_decode_claim_checkpoint_v1(&trailing).is_err());
        let mut duplicated: Vec<_> = c.records.iter().collect();
        duplicated.insert(0, duplicated[0]);
        let noncanonical = borsh::to_vec(&(c.point, duplicated, &c.liability_state, &c.material)).unwrap();
        assert!(palw_decode_claim_checkpoint_v1(&noncanonical).is_err(), "duplicate record keys cannot disappear during decode");
        let reversed: Vec<_> = c.records.iter().rev().collect();
        let noncanonical = borsh::to_vec(&(c.point, reversed, &c.liability_state, &c.material)).unwrap();
        assert!(palw_decode_claim_checkpoint_v1(&noncanonical).is_err(), "checkpoint map order is canonical");
    }
}

impl crate::config::params::Params {
    pub fn sync_palw_claim_local_settlement_v1(&mut self) {
        use crate::config::params::ForkActivation;
        let from = self.palw_claim_local_settlement_v1.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score());
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_claim_local_settlement_from_daa(from);
        }
    }
    pub fn validate_palw_claim_local_settlement_v1(&self) -> Result<(), crate::palw_mode_v2::PalwModeV2Error> {
        use crate::{
            config::params::ForkActivation,
            palw_mode_v2::{PalwConsensusMode, PalwModeV2Error},
        };
        let from = self.palw_claim_local_settlement_v1.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score());
        let mirror = match &self.palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(bundle) => bundle.state.claim_local_settlement_from_daa(),
            _ => None,
        };
        if from != mirror {
            return Err(PalwModeV2Error::Invalid("claim-local settlement fence disagrees with its state mirror"));
        }
        if from.is_some() {
            return Err(PalwModeV2Error::Invalid(
                "RFC-0023 stage 2 cannot arm: runtime integration is incomplete and material retention / transition drills are unmeasured",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod fence_tests {
    use crate::config::params::{
        DEVNET_PARAMS, ForkActivation, MAINNET_PARAMS, SIMNET_PARAMS, TESTNET_PARAMS, palw_t12_shipped_params,
    };
    #[test]
    fn rfc23s2_fence_is_dormant_some_only_and_never_armable_without_measurements() {
        for mut p in [MAINNET_PARAMS, TESTNET_PARAMS, SIMNET_PARAMS, DEVNET_PARAMS, palw_t12_shipped_params()] {
            assert!(p.palw_claim_local_settlement_v1.is_none());
            p.validate_palw_claim_local_settlement_v1().unwrap();
            assert!(p.palw_fences_v1().contains(&("palw_claim_local_settlement_v1", None)));
            let old = p.consensus_params_id();
            let identity = p.consensus_identity_id();
            p.palw_claim_local_settlement_v1 = Some(ForkActivation::never());
            p.sync_palw_claim_local_settlement_v1();
            p.validate_palw_claim_local_settlement_v1().unwrap();
            assert_eq!(p.consensus_params_id(), old);
            assert_eq!(p.consensus_identity_id(), identity);
            p.palw_claim_local_settlement_v1 = Some(ForkActivation::new(100));
            p.sync_palw_claim_local_settlement_v1();
            assert_ne!(p.consensus_params_id(), old);
            assert!(p.validate_palw_claim_local_settlement_v1().is_err());
        }
    }
}

#[cfg(test)]
mod local_capacity_classification_tests {
    use super::*;
    #[test]
    fn rfc23s2_scanner_capacity_refusal_is_never_a_semantic_verdict() {
        use crate::palw_native_resource_v1::NativeDecodeErrorV1;
        use std::io::{Error, ErrorKind};
        for kind in [ErrorKind::OutOfMemory, ErrorKind::TimedOut, ErrorKind::Unsupported] {
            assert!(matches!(
                claim_local_decode_io_v1::<PalwClaimLiabilityFrameV1>(Error::from(kind)),
                NativeDecodeErrorV1::Resource(_)
            ));
            assert!(matches!(claim_local_checkpoint_io_v1(Error::from(kind), "malformed"), PalwClaimCheckpointErrorV1::Resource(_)));
        }
        for kind in [ErrorKind::InvalidData, ErrorKind::UnexpectedEof] {
            assert!(matches!(
                claim_local_decode_io_v1::<PalwClaimLiabilityFrameV1>(Error::from(kind)),
                NativeDecodeErrorV1::Decode(_)
            ));
            assert!(matches!(
                claim_local_checkpoint_io_v1(Error::from(kind), "malformed"),
                PalwClaimCheckpointErrorV1::Semantic("malformed")
            ));
        }
    }
}
