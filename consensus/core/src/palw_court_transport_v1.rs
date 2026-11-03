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
    if let Some((cost, _, _, _)) =
        family_material_accounted_v2(state, params, now, object).map_err(|_| "native family material transport accounting refused")?
    {
        return Ok(Some((cost, false)));
    }
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

/// New signed cohort only. The existing session move's weight represents one
/// indivisible service-control slot; source work/io remains its pinned packet
/// cost in service supply accounting. This classification is shared by the
/// acceptance walk and block fold, so multiple material controls cannot evade
/// the actual one-control budget by being signed or split into small bodies.
pub(crate) fn family_material_accounted_v2(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    now: u64,
    object: &PalwConsensusObjectV2,
) -> Result<Option<(crate::palw_court_capacity_v1::PalwCourtCostV1, crate::palw_court_service_v1::ServiceCost, bool, Hash64)>, String>
{
    use crate::palw_court_family_material_v1 as m;
    use crate::palw_court_service_v1::ServiceCost;
    let PalwConsensusObjectV2::CourtFamilyMaterialV1 { control: s } = object else { return Ok(None) };
    if !params.family_supplier_active_at(now) {
        return Ok(None);
    }
    use borsh::BorshDeserialize;
    let key = (crate::palw_court_family_supplier_v1::FAMILY_SUPPLIER_TABLE_V1, s.contract.as_byte_slice().to_vec());
    let raw = state.kernel_route_state_v1().and_then(|r| r.aux.get(&key)).ok_or("family transport contract missing")?;
    // Fixed-size terms and the borrowed signatures/catalogue header only. The
    // cost classifier must never turn an allocation refusal into invalid input.
    let mut reader = raw.as_slice();
    let version = u16::deserialize_reader(&mut reader).map_err(|e| e.to_string())?;
    let terms =
        crate::palw_court_family_supplier_v1::FamilySupplierTermsV1::deserialize_reader(&mut reader).map_err(|e| e.to_string())?;
    if version != 2 || terms.signing_message_v1()? != s.contract {
        return Err("family transport contract identity".into());
    }
    if crate::palw_court_family_supplier_v1::signed_profile_generation_v3(&terms)? == 1 {
        return Ok(None);
    }
    for _ in 0..2 {
        let n = u32::deserialize_reader(&mut reader).map_err(|e| e.to_string())? as usize;
        if n != crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN {
            return Err("family transport consent signature width".into());
        }
        reader = reader.get(n..).ok_or("short family transport consent")?;
    }
    let allocation = Hash64::deserialize_reader(&mut reader).map_err(|e| e.to_string())?;
    let source = crate::palw_g14_values_v1::PALW_G14_SOURCE_PACKET_COST_V1;
    let bytes = payload_bytes(object).map_err(str::to_owned)?;
    let service = ServiceCost { bytes: bytes.max(source.bytes), ..source };
    let policy = params.court_capacity_at_v1(now).unwrap_or(crate::palw_g14_values_v1::PALW_G14_COURT_CAPACITY_POLICY_ESTIMATE_V1);
    let mut memory = bytes.checked_mul(policy.memory_per_byte).ok_or("family transport memory overflow")?;
    let (draft, ordinal) = match &s.action {
        m::FamilyMaterialActionV1::Begin { .. } => (None, Some(0)),
        m::FamilyMaterialActionV1::BeginReserved { .. } => (None, None),
        m::FamilyMaterialActionV1::Part { draft, offset, .. } => {
            (Some(*draft), Some(offset / (m::FAMILY_MATERIAL_PART_BYTES_V1 as u64) + 1))
        }
        m::FamilyMaterialActionV1::Finish { draft } => {
            let key = m::material_aux_key_v1(1, *draft, None);
            let raw = state
                .kernel_route_state_v1()
                .and_then(|r| r.aux.get(&(m::FAMILY_MATERIAL_TABLE_V1, key)))
                .ok_or("family Finish draft missing")?;
            // Fixed native row: decoding allocates no vector or untrusted count.
            let d = borsh::from_slice::<m::FamilyMaterialDraftV1>(raw).map_err(|e| e.to_string())?;
            memory = memory
                .checked_add(d.total_bytes.checked_mul(2).ok_or("family Finish memory overflow")?)
                .ok_or("family Finish memory overflow")?;
            (Some(*draft), Some(d.total_bytes.div_ceil(m::FAMILY_MATERIAL_PART_BYTES_V1 as u64) + 1))
        }
    };
    let target =
        crate::palw_court_shared_tir_stream_v1::stream_digest(b"misaka-palw/family-material/target/v1", &(terms.inventory, s.target))?;
    // The native catalogue reserves every upper-bound Part and places Finish
    // last. An actual body may need fewer Parts; it must still use that rooted
    // Finish opportunity rather than reclassifying an unused tail Part.
    let primary_ordinal = if matches!(s.action, m::FamilyMaterialActionV1::Finish { .. }) {
        u64::deserialize_reader(&mut reader).map_err(|e| e.to_string())?; // accepted DAA
        let count = u32::deserialize_reader(&mut reader).map_err(|e| e.to_string())?;
        let mut found = None;
        for _ in 0..count {
            let id = Hash64::deserialize_reader(&mut reader).map_err(|e| e.to_string())?;
            u64::deserialize_reader(&mut reader).map_err(|e| e.to_string())?; // native bound
            let packets = u32::deserialize_reader(&mut reader).map_err(|e| e.to_string())?;
            if packets < 3 {
                return Err("family transport native target packet shape".into());
            }
            let skip =
                usize::try_from(packets).map_err(|e| e.to_string())?.checked_mul(64).ok_or("family transport catalogue overflow")?;
            reader = reader.get(skip..).ok_or("family transport short target catalogue")?;
            if id == target && found.replace(u64::from(packets) - 1).is_some() {
                return Err("family transport duplicate native target".into());
            }
        }
        Some(found.ok_or("family transport Finish native target absent")?)
    } else {
        ordinal
    };
    let mut packet = primary_ordinal
        .map(|ordinal| {
            crate::palw_court_shared_tir_stream_v1::stream_digest(
                b"misaka-palw/family-supplier/service-packet/v1",
                &(s.contract, allocation, target, ordinal),
            )
            .map(|id| id.as_bytes())
        })
        .transpose()?;
    if let Some(draft) = draft {
        let key = m::material_aux_key_v1(5, draft, None);
        if let Some(raw) = state.kernel_route_state_v1().and_then(|r| r.aux.get(&(m::FAMILY_MATERIAL_TABLE_V1, key))) {
            let mut reader = raw.as_slice();
            let contract = Hash64::deserialize_reader(&mut reader).map_err(|e| e.to_string())?;
            let version = u16::deserialize_reader(&mut reader).map_err(|e| e.to_string())?;
            let claim = Hash64::deserialize_reader(&mut reader).map_err(|e| e.to_string())?;
            let identity = Hash64::deserialize_reader(&mut reader).map_err(|e| e.to_string())?;
            for _ in 0..3 {
                u64::deserialize_reader(&mut reader).map_err(|e| e.to_string())?;
            }
            let quote = m::FamilyDraftQuoteV2::deserialize_reader(&mut reader).map_err(|e| e.to_string())?;
            if version != 2 || contract != s.contract || claim != terms.claim || identity != draft {
                return Err("family transport independent draft scope".into());
            }
            if let Some(ordinal) = ordinal {
                let ordinal = ordinal.checked_sub(1).ok_or("independent draft transfer ordinal")?;
                packet = Some(
                    crate::palw_court_shared_tir_stream_v1::stream_digest(
                        b"misaka-palw/family-draft/service/v2",
                        &(identity, quote, ordinal),
                    )?
                    .as_bytes(),
                );
            }
        }
    }
    let reserved = packet.and_then(|id| state.court_service_state_v1().map(|book| (id, book))).is_some_and(|(id, book)| {
        book.book.assigned_daa(&id) == Some(now)
            && book.book.packet(&id).is_some_and(|p| {
                service.bytes <= p.cost.bytes
                    && service.work <= p.cost.work
                    && service.io <= p.cost.io
                    && service.controls <= p.cost.controls
            })
    });
    let cost = crate::palw_court_capacity_v1::PalwCourtCostV1 { bytes, work: policy.block_budget.work, memory };
    let message = s.signing_message_v1(terms.network)?;
    Ok(Some((cost, service, reserved, message)))
}
