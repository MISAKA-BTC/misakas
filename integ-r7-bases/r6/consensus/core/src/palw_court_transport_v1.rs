//! RFC-0014/0026 transport accounting. This is a separate dormant fence: no
//! existing capacity-policy ID or operation class is reinterpreted. It charges
//! TIR public controls/grants to the SAME session budget in ingress and fold.
//! No preset arms it. Real-network activation is refused until measurements,
//! the shared pursuit envelope and the physical budget split are reviewed.
use crate::Hash64;
use crate::config::params::{ForkActivation, Params};
use crate::palw_court_capacity_v1::{PalwCourtCostV1, PalwCourtOpClassV1};
use crate::palw_mode_v2::{PalwConsensusMode, PalwModeV2Error};
use crate::palw_state_v2::{PalwChainStateV2, PalwConsensusObjectV2, PalwStateParamsV2};
use borsh::{BorshDeserialize, BorshSerialize};
use std::io::{self, Write};

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwCourtTransportPolicyV1 {
    pub version: u16,
    /// Same deterministic work scale as court capacity; not runtime CPU time.
    pub base_work: u64,
    pub work_per_payload_byte: u64,
    /// Covers state-dependent resident/assembly memory, not just wire bytes.
    pub base_memory: u64,
    pub memory_per_payload_byte: u64,
}
impl PalwCourtTransportPolicyV1 {
    pub fn validate(self) -> Result<(), &'static str> {
        if self.version != 1
            || self.base_work == 0
            || self.work_per_payload_byte == 0
            || self.base_memory == 0
            || self.memory_per_payload_byte == 0
        {
            return Err("court transport needs a version and positive reviewed work/memory bounds");
        }
        Ok(())
    }
    pub fn id(self) -> Hash64 {
        let bytes = borsh::to_vec(&self).expect("fixed-size policy serializes");
        Hash64::from_bytes(
            blake2b_simd::Params::new()
                .hash_length(64)
                .key(b"misaka-palw/court-transport/v1")
                .hash(&bytes)
                .as_bytes()
                .try_into()
                .unwrap(),
        )
    }
    fn cost(self, bytes: u64) -> Result<PalwCourtCostV1, &'static str> {
        self.validate()?;
        Ok(PalwCourtCostV1 {
            bytes,
            work: self
                .work_per_payload_byte
                .checked_mul(bytes)
                .and_then(|x| x.checked_add(self.base_work))
                .ok_or("court transport work overflows")?,
            memory: self
                .memory_per_payload_byte
                .checked_mul(bytes)
                .and_then(|x| x.checked_add(self.base_memory))
                .ok_or("court transport memory overflows")?,
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwCourtTransportFenceV1 {
    pub activation: ForkActivation,
    pub policy: PalwCourtTransportPolicyV1,
}
impl PalwCourtTransportFenceV1 {
    /// Fork-ID/test probe only, NEVER a production measurement or preset.
    pub fn unapproved_probe_at(activation: ForkActivation) -> Self {
        Self {
            activation,
            policy: PalwCourtTransportPolicyV1 {
                version: 1,
                base_work: 1,
                work_per_payload_byte: 1,
                base_memory: 1,
                memory_per_payload_byte: 1,
            },
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwCourtTransportMirrorV1 {
    pub from_daa: u64,
    pub policy: PalwCourtTransportPolicyV1,
}
impl Params {
    pub fn sync_palw_court_transport_v1(&mut self) {
        let mirror = self
            .palw_court_transport_v1
            .filter(|f| f.activation != ForkActivation::never())
            .map(|f| PalwCourtTransportMirrorV1 { from_daa: f.activation.daa_score(), policy: f.policy });
        if let PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_court_transport_v1(mirror);
        }
    }
    pub fn validate_palw_court_transport_v1(&self) -> Result<(), PalwModeV2Error> {
        let armed = self.palw_court_transport_v1.filter(|f| f.activation != ForkActivation::never());
        if let PalwConsensusMode::ConsensusV2(bundle) = &self.palw_consensus_mode {
            let want = armed.map(|f| PalwCourtTransportMirrorV1 { from_daa: f.activation.daa_score(), policy: f.policy });
            if bundle.state.court_transport_v1() != want {
                return Err(PalwModeV2Error::Invalid("court transport disagrees with its state mirror"));
            }
        }
        let Some(fence) = armed else { return Ok(()) };
        if !matches!(self.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)) {
            return Err(PalwModeV2Error::Invalid("court transport requires ConsensusV2"));
        }
        let at = fence.activation.daa_score();
        if !self.palw_court_capacity_v1.is_some_and(|f| f.activation != ForkActivation::never() && f.activation.daa_score() <= at)
            || !self.palw_tir_public_conviction_v1.is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= at)
        {
            return Err(PalwModeV2Error::Invalid("court transport needs court capacity and the TIR public route at or below it"));
        }
        fence.policy.validate().map_err(PalwModeV2Error::Invalid)?;
        if !self.has_isolated_g14_genesis_v1() {
            return Err(PalwModeV2Error::Invalid(
                "court transport cannot be armed: no reviewed transport measurement/physical budget record; OneOutsiderCapacityIncomplete",
            ));
        }
        Ok(())
    }
}

/// Count actual canonical lifecycle payload bytes without allocating a second
/// body buffer. The 2-byte payload version is included, as is the object tag,
/// signatures, metadata and every response/witness part. Transaction framing
/// stays in ordinary transaction mass and is NOT claimed as measured here.
fn payload_bytes(object: &PalwConsensusObjectV2) -> Result<u64, &'static str> {
    struct Counter(u64);
    impl Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0 = self.0.checked_add(bytes.len() as u64).ok_or_else(|| io::Error::other("encoded payload size overflows"))?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut count = Counter(2);
    object.serialize(&mut count).map_err(|_| "court transport payload serialization failed")?;
    Ok(count.0)
}

pub fn palw_court_transport_cost_v1(
    state: &PalwChainStateV2,
    mirror: Option<PalwCourtTransportMirrorV1>,
    now: u64,
    object: &PalwConsensusObjectV2,
) -> Result<Option<PalwCourtCostV1>, &'static str> {
    let Some(mirror) = mirror.filter(|m| now >= m.from_daa) else { return Ok(None) };
    let claim = match object {
        PalwConsensusObjectV2::TirPublicControlV1 { control } => control.claim,
        PalwConsensusObjectV2::TirPublicGrantReadyV1 { grant } => grant.claim,
        _ => return Ok(None),
    };
    // Old claims retain their old transport rules even after activation. Retired
    // native claims still have their immutable ORIGINAL frame acceptance clock.
    let accepted = crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state, claim)
        .map(|frame| frame.plan_v1().terms.accepted_daa)
        .or_else(|| state.claim(&claim).map(|c| c.accepted_daa));
    if accepted.is_some_and(|daa| daa < mirror.from_daa) {
        return Ok(None);
    }
    // An unknown-claim control is not free to parse; successful application and
    // local pre-auth work limits remain separate checks in the native ingress.
    Ok(Some(mirror.policy.cost(payload_bytes(object)?)?))
}

/// The ONE classification/cost path used by both ingress and the fold. The
/// existing one-move/session split stays intact; transport uses session budget.
pub fn palw_court_accounted_op_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    now: u64,
    object: &PalwConsensusObjectV2,
) -> Result<Option<(PalwCourtCostV1, bool)>, &'static str> {
    let transport = palw_court_transport_cost_v1(state, params.court_transport_v1(), now, object)?;
    let policy = params.court_capacity_at_v1(now);
    if transport.is_some() && policy.is_none() {
        return Err("court transport has no active session capacity budget");
    }
    let ordinary = policy.as_ref().and_then(|p| crate::palw_court_capacity_v1::palw_court_op_cost_v1(p, state, object));
    match (ordinary, transport) {
        (Some(_), Some(_)) => Err("court transport overlaps an existing operation class; a new explicit cost profile is required"),
        (Some(cost), None) => Ok(Some((
            cost,
            matches!(crate::palw_court_capacity_v1::palw_court_op_class_v1(state, object), Some(PalwCourtOpClassV1::OneMove)),
        ))),
        (None, Some(cost)) => Ok(Some((cost, false))),
        (None, None) => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_lifecycle_objects_v2::{PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2};
    use crate::palw_tir_public_carriage_v1::{PalwTirPublicControlActionV1, palw_tir_public_control_fixture_v1};
    use crate::palw_tir_public_owned_v1::PalwTirPublicOwnedActionV1;

    fn part(bytes: usize) -> PalwConsensusObjectV2 {
        // Codec geometry only; this fixture is not a native signed admission.
        let mut control = palw_tir_public_control_fixture_v1();
        control.action = PalwTirPublicControlActionV1::Acquisition(PalwTirPublicOwnedActionV1::Part {
            demand_id: Hash64::from_u64_word(55),
            answerer: control.signer,
            offset: 0,
            bytes: vec![9; bytes],
        });
        PalwConsensusObjectV2::TirPublicControlV1 { control: Box::new(control) }
    }
    #[test]
    fn transport_codec_cost_is_exact_and_dormant_before_its_fence() {
        let state = PalwChainStateV2::genesis();
        let mirror = PalwCourtTransportMirrorV1 {
            from_daa: 100,
            policy: PalwCourtTransportFenceV1::unapproved_probe_at(ForkActivation::never()).policy,
        };
        let a = part(13);
        let expected = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object: a.clone() })
            .unwrap()
            .len() as u64;
        let cost = palw_court_transport_cost_v1(&state, Some(mirror), 100, &a).unwrap().unwrap();
        assert_eq!(cost.bytes, expected);
        assert_eq!(cost.work, expected + 1);
        assert_eq!(cost.memory, expected + 1);
        let b = palw_court_transport_cost_v1(&state, Some(mirror), 100, &part(1013)).unwrap().unwrap();
        assert_eq!(b.bytes - cost.bytes, 1000);
        assert_eq!(b.work - cost.work, 1000);
        assert_eq!(b.memory - cost.memory, 1000);
        let invalid = PalwCourtTransportMirrorV1 { policy: PalwCourtTransportPolicyV1 { version: 0, ..mirror.policy }, ..mirror };
        assert_eq!(palw_court_transport_cost_v1(&state, Some(invalid), 99, &a), Ok(None));
        assert_eq!(palw_court_transport_cost_v1(&state, None, 100, &a), Ok(None));
        assert!(palw_court_transport_cost_v1(&state, Some(invalid), 100, &a).is_err());
    }
    #[test]
    fn transport_cost_and_accumulation_overflows_refuse() {
        let mut policy = PalwCourtTransportFenceV1::unapproved_probe_at(ForkActivation::never()).policy;
        policy.work_per_payload_byte = u64::MAX;
        assert!(policy.cost(2).is_err());
        policy.work_per_payload_byte = 1;
        policy.memory_per_payload_byte = u64::MAX;
        assert!(policy.cost(2).is_err());
        for full in [
            PalwCourtCostV1 { work: u64::MAX, bytes: 0, memory: 0 },
            PalwCourtCostV1 { work: 0, bytes: u64::MAX, memory: 0 },
            PalwCourtCostV1 { work: 0, bytes: 0, memory: u64::MAX },
        ] {
            assert!(full.checked_add(PalwCourtCostV1 { work: 1, bytes: 1, memory: 1 }).is_none());
        }
    }
}
