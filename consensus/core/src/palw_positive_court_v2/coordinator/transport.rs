//! Private, bounded fragment carriage. This defines no native wire tag or
//! signature domain. The descriptor is transport metadata, NOT evidence of
//! correctness or authorship; only the existing Node checker advances coverage.
use super::{EncodedPartV2, ErrorV2, Hash64, PositiveCoordinatorV2, WitnessStatusV2};

// Conservative local envelope allowance, not a proposed network codec size.
const ENVELOPE_BYTES: u64 = 192;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MaterialTransportLimitsV2 {
    pub max_chunk_bytes: u64,
    pub max_buffer_bytes: u64,
    pub max_wire_bytes: u64,
    pub max_packets: u64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MaterialTransportMeterV2 {
    pub wire_bytes: u64,
    pub packets: u64,
    pub checked_parts: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MaterialDescriptorV2 {
    pub witness: Hash64,
    pub index: u64,
    pub cone_bytes: u64,
    pub trace_bytes: Option<u64>,
    pub payload: Hash64,
}
impl MaterialDescriptorV2 {
    pub fn for_part(witness: Hash64, index: u64, cone: &[u8], trace: Option<&[u8]>) -> Self {
        Self {
            witness,
            index,
            cone_bytes: cone.len() as u64,
            trace_bytes: trace.map(|x| x.len() as u64),
            payload: payload_id(witness, index, cone, trace),
        }
    }
    fn length(self) -> Result<u64, ErrorV2> {
        self.cone_bytes.checked_add(self.trace_bytes.unwrap_or(0)).ok_or(ErrorV2::Budget)
    }
}
fn payload_id(witness: Hash64, index: u64, cone: &[u8], trace: Option<&[u8]>) -> Hash64 {
    // Local content identity only. Never accepted as a computation certificate.
    // Hash slices incrementally: serializing a second full payload would exceed
    // the advertised single-buffer memory bound.
    let mut state = blake2b_simd::Params::new().hash_length(64).key(b"misaka/private-material-part/v2").to_state();
    state.update(witness.as_byte_slice());
    state.update(&index.to_le_bytes());
    state.update(&(cone.len() as u64).to_le_bytes());
    state.update(cone);
    match trace {
        None => {
            state.update(&[0]);
        }
        Some(trace) => {
            state.update(&[1]);
            state.update(&(trace.len() as u64).to_le_bytes());
            state.update(trace);
        }
    }
    Hash64::from_bytes(state.finalize().as_bytes().try_into().expect("64"))
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterialEffectV2 {
    Buffered,
    Duplicate,
    Checked,
    AlreadyChecked,
}
struct Buffer {
    descriptor: MaterialDescriptorV2,
    bytes: Vec<u8>,
}
pub(super) struct MaterialTransportV2 {
    limits: MaterialTransportLimitsV2,
    meter: MaterialTransportMeterV2,
    buffer: Option<Buffer>,
    last_checked: Option<MaterialDescriptorV2>,
}
impl PositiveCoordinatorV2 {
    /// Configure once, before any candidate. Reserve the wire allowance IN
    /// ADDITION to all candidates' checker payloads, within admission's ceiling.
    /// Replacing relays, candidates,
    /// or discarding a partial buffer never creates a fresh transport allowance.
    pub fn configure_material_transport(&mut self, limits: MaterialTransportLimitsV2) -> Result<(), ErrorV2> {
        if self.claim != super::ClaimStatusV2::Checking {
            return Err(ErrorV2::Closed);
        }
        if self.transport.is_some() || self.meter.attempts != 0 {
            return Err(ErrorV2::Order);
        }
        if limits.max_chunk_bytes == 0
            || limits.max_buffer_bytes == 0
            || limits.max_packets == 0
            || limits.max_wire_bytes == 0
            || limits.max_chunk_bytes > self.limits.max_batch_bytes
            || limits.max_buffer_bytes > self.policy.max_part_bytes
        {
            return Err(ErrorV2::Budget);
        }
        let reservation = self.meter.reserved_witness_bytes.checked_add(limits.max_wire_bytes).ok_or(ErrorV2::Budget)?;
        if reservation > self.limits.max_reserved_witness_bytes {
            return Err(ErrorV2::Budget);
        }
        self.transport =
            Some(MaterialTransportV2 { limits, meter: MaterialTransportMeterV2::default(), buffer: None, last_checked: None });
        self.meter.reserved_witness_bytes = reservation;
        Ok(())
    }
    pub fn material_transport_meter(&self) -> Option<MaterialTransportMeterV2> {
        self.transport.as_ref().map(|x| x.meter)
    }
    /// Administrative abandonment of an unsigned transport envelope. No witness
    /// rejection, Miner fault, refund, attempt reset, or deadline extension.
    pub fn discard_material_buffer(&mut self) {
        if let Some(transport) = &mut self.transport {
            transport.buffer = None;
        }
    }
    /// Any relay may resume the same descriptor. Ordered, contiguous fragments
    /// are required; an identical fragment within the buffered prefix is a
    /// duplicate. Failed/conflicting packets preserve the accepted prefix.
    pub fn submit_material_chunk(
        &mut self,
        descriptor: MaterialDescriptorV2,
        offset: u64,
        bytes: &[u8],
        now: u64,
    ) -> Result<MaterialEffectV2, ErrorV2> {
        self.ensure_open(now)?;
        let transport = self.transport.as_mut().ok_or(ErrorV2::UnsupportedProfile)?;
        if bytes.is_empty() || bytes.len() as u64 > transport.limits.max_chunk_bytes {
            return Err(ErrorV2::Budget);
        }
        let wire = (bytes.len() as u64).checked_add(ENVELOPE_BYTES).ok_or(ErrorV2::Budget)?;
        let wire_bytes = transport.meter.wire_bytes.checked_add(wire).ok_or(ErrorV2::Budget)?;
        let packets = transport.meter.packets.checked_add(1).ok_or(ErrorV2::Budget)?;
        if wire_bytes > transport.limits.max_wire_bytes || packets > transport.limits.max_packets {
            return Err(ErrorV2::Budget);
        }
        // Charge retransmission and rejected bounded envelopes, not only unique
        // accepted data. All mutation below keeps these cumulative counters.
        transport.meter.wire_bytes = wire_bytes;
        transport.meter.packets = packets;
        let (_, case) = self.current.as_ref().ok_or(ErrorV2::Order)?;
        if case.witness().id() != descriptor.witness {
            return Err(ErrorV2::Binding);
        }
        if transport.last_checked == Some(descriptor) && descriptor.index.checked_add(1) == Some(case.cursor()) {
            // Acknowledges existing checked coverage; does not authenticate the
            // retransmitted packet or run the arithmetic checker a second time.
            return Ok(MaterialEffectV2::AlreadyChecked);
        }
        if case.statuses().1 != WitnessStatusV2::Checking {
            return Err(ErrorV2::Closed);
        }
        if descriptor.index != case.cursor() {
            return Err(ErrorV2::Order);
        }
        let length = descriptor.length()?;
        if descriptor.cone_bytes == 0 || length > transport.limits.max_buffer_bytes {
            return Err(ErrorV2::Budget);
        }
        let end = offset.checked_add(bytes.len() as u64).filter(|n| *n <= length).ok_or(ErrorV2::Order)?;
        if let Some(buffer) = &transport.buffer {
            // Candidate replacement abandons only its temporary bytes.
            if buffer.descriptor.witness != descriptor.witness || buffer.descriptor.index != descriptor.index {
                transport.buffer = None;
            } else if buffer.descriptor != descriptor {
                return Err(ErrorV2::Binding);
            }
        }
        if transport.buffer.is_none() {
            if offset != 0 {
                return Err(ErrorV2::Order);
            }
            let mut buffer = Vec::new();
            buffer.try_reserve_exact(usize::try_from(length).map_err(|_| ErrorV2::Budget)?).map_err(|_| ErrorV2::Budget)?;
            transport.buffer = Some(Buffer { descriptor, bytes: buffer });
        }
        let buffer = transport.buffer.as_mut().expect("initialized above");
        let next = buffer.bytes.len() as u64;
        if offset < next {
            if end <= next && buffer.bytes[offset as usize..end as usize] == *bytes {
                return Ok(MaterialEffectV2::Duplicate);
            }
            return Err(ErrorV2::Binding);
        }
        if offset != next {
            return Err(ErrorV2::Order);
        }
        buffer.bytes.extend_from_slice(bytes);
        if end != length {
            return Ok(MaterialEffectV2::Buffered);
        }
        let buffer = transport.buffer.take().expect("complete buffer");
        let cone_end = usize::try_from(descriptor.cone_bytes).map_err(|_| ErrorV2::Budget)?;
        let cone = &buffer.bytes[..cone_end];
        let trace = descriptor.trace_bytes.map(|_| &buffer.bytes[cone_end..]);
        if payload_id(descriptor.witness, descriptor.index, cone, trace) != descriptor.payload {
            // An unsigned corrupt transport cannot reject a signed candidate.
            return Err(ErrorV2::Binding);
        }
        self.submit_batch(descriptor.witness, descriptor.index, &[EncodedPartV2 { cone, trace }], now)?;
        let transport = self.transport.as_mut().expect("configured above");
        transport.last_checked = Some(descriptor);
        transport.meter.checked_parts += 1;
        Ok(MaterialEffectV2::Checked)
    }
}
