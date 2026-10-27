//! Private canonical replay from complete borrowed operands. This authenticates
//! only the existing primitive's local finding; the adapter must reconstruct the
//! initial native contract from actual state. No wire, paid receipt, old move
//! equivalence, retention certificate or whole-original completion follows.
use super::{PalwTokenizerStreamPacketV1 as Packet, PalwTokenizerTableStreamV1 as Court};
use crate::{Hash64, palw_json_key_set_v1::PalwJsonKeyCompactProofV1 as Proof};
#[derive(Clone, Copy)]
pub(crate) struct Material<'a> {
    pub(crate) raw: &'a [u8],
    pub(crate) proofs: &'a [Proof],
    pub(crate) rendering: Option<&'a [u8]>,
}
#[derive(Clone, Copy)]
pub(crate) struct Budget {
    pub(crate) max_material_bytes: u64,
    pub(crate) max_proofs: u64,
    pub(crate) max_native_calls: u64,
    pub(crate) max_packets: u64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Work {
    pub(crate) material_bytes: u64,
    pub(crate) native_calls: u64,
    pub(crate) packets: u64,
    pub(crate) proofs: u64,
}
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Refusal {
    InitialState,
    OperandLength,
    RenderingOperand,
    Budget,
    NeedProof { index: usize, namespace: u64, key: Hash64 },
    ExtraProof { first_unused: usize },
    ProofTooLarge,
    NoProgress,
    Native(String),
}
pub(crate) struct Finding {
    violation: bool,
    key_root: Hash64,
    key_events: u64,
    work: Work,
}
impl Finding {
    pub(crate) fn violation(&self) -> bool {
        self.violation
    }
    pub(crate) fn key_root(&self) -> Hash64 {
        self.key_root
    }
    pub(crate) fn key_events(&self) -> u64 {
        self.key_events
    }
    pub(crate) fn work(&self) -> Work {
        self.work
    }
}
fn call(work: &mut Work, budget: Budget) -> Result<(), Refusal> {
    work.native_calls = work.native_calls.checked_add(1).ok_or(Refusal::Budget)?;
    if work.native_calls > budget.max_native_calls {
        return Err(Refusal::Budget);
    }
    Ok(())
}
fn native<T>(r: Result<T, String>) -> Result<T, Refusal> {
    r.map_err(Refusal::Native)
}
/// Borrowed input and base remain unchanged on every return. Clone/probe/parser
/// scratch and input ownership must be admitted separately by the native caller.
/// Greedy packet packing uses the exact existing grammar's 65536 raw bytes,
/// 64 proofs and 90000 Borsh bytes. These are not new total-original limits.
pub(crate) fn build(base: &Court, input: Material<'_>, budget: Budget) -> Result<Finding, Refusal> {
    native(base.check())?;
    if base.tokenizer_offset() != 0
        || base.key_events() != 0
        || base.pending_key().is_some()
        || base.tokenizer_sealed_v1()
        || base.rendering_offset() != 0
    {
        return Err(Refusal::InitialState);
    }
    if input.raw.len() as u64 != base.operand_raw_length {
        return Err(Refusal::OperandLength);
    }
    if input.rendering.is_some() != base.table.present || input.rendering.map_or(0, <[u8]>::len) as u64 != u64::from(base.table.length)
    {
        return Err(Refusal::RenderingOperand);
    }
    let mut work = Work::default();
    work.proofs = u64::try_from(input.proofs.len()).map_err(|_| Refusal::Budget)?;
    if work.proofs > budget.max_proofs {
        return Err(Refusal::Budget);
    }
    work.material_bytes =
        (input.raw.len() as u64).checked_add(input.rendering.map_or(0, <[u8]>::len) as u64).ok_or(Refusal::Budget)?;
    for proof in input.proofs {
        // Refuse before expansion/allocation or scanning an oversized sibling Vec.
        if proof.siblings.len() > crate::palw_json_key_set_v1::PALW_JSON_KEY_SET_DEPTH_V1 {
            return Err(Refusal::ProofTooLarge);
        }
        work.material_bytes = work
            .material_bytes
            .checked_add(borsh::object_length(proof).map_err(|e| Refusal::Native(e.to_string()))? as u64)
            .ok_or(Refusal::Budget)?;
        if work.material_bytes > budget.max_material_bytes {
            return Err(Refusal::Budget);
        }
    }
    if work.material_bytes > budget.max_material_bytes {
        return Err(Refusal::Budget);
    }
    let mut c = base.clone();
    let mut proof_index = 0usize;
    loop {
        let mut probe = c.clone();
        let mut packet = Packet { offset: c.tokenizer_offset(), bytes: Vec::new(), proofs: Vec::new() };
        let mut carried = 16usize; // offset + two Borsh Vec<u32 length> prefixes.
        loop {
            call(&mut work, budget)?;
            native(probe.pending_overrun())?;
            if let Some((namespace, key)) = probe.pending_key() {
                let proof = input.proofs.get(proof_index).ok_or(Refusal::NeedProof { index: proof_index, namespace, key })?;
                let size = borsh::object_length(proof).map_err(|e| Refusal::Native(e.to_string()))?;
                let fits = packet.proofs.len() < 64 && carried.checked_add(size).is_some_and(|n| n <= 90000);
                if !fits {
                    if packet.bytes.is_empty() && packet.proofs.is_empty() {
                        return Err(Refusal::ProofTooLarge);
                    }
                    break;
                }
                // Real compact expansion, parser pending namespace/key and old
                // key-set root are all verified by the native packet method.
                let atom = Packet { offset: probe.tokenizer_offset(), bytes: Vec::new(), proofs: vec![proof.clone()] };
                call(&mut work, budget)?;
                native(probe.packet(base.context, &atom))?;
                packet.proofs.push(proof.clone());
                carried = carried.checked_add(size).ok_or(Refusal::Budget)?;
                proof_index = proof_index.checked_add(1).ok_or(Refusal::Budget)?;
            } else {
                let offset = usize::try_from(probe.tokenizer_offset()).map_err(|_| Refusal::OperandLength)?;
                if offset == input.raw.len() {
                    break;
                }
                let room = (65536usize - packet.bytes.len()).min(90000usize - carried);
                if room == 0 {
                    break;
                }
                let end = offset.checked_add(room).ok_or(Refusal::Budget)?.min(input.raw.len());
                let mut parser = probe.parser.clone();
                call(&mut work, budget)?;
                let consumed = native(parser.push(base.context, probe.tokenizer_offset(), &input.raw[offset..end]))?;
                if consumed == 0 || consumed > end - offset {
                    return Err(Refusal::NoProgress);
                }
                let atom = Packet {
                    offset: probe.tokenizer_offset(),
                    bytes: input.raw[offset..offset + consumed].to_vec(),
                    proofs: Vec::new(),
                };
                call(&mut work, budget)?;
                native(probe.packet(base.context, &atom))?;
                packet.bytes.extend_from_slice(&atom.bytes);
                carried = carried.checked_add(consumed).ok_or(Refusal::Budget)?;
            }
        }
        if packet.bytes.is_empty() && packet.proofs.is_empty() {
            break;
        }
        work.packets = work.packets.checked_add(1).ok_or(Refusal::Budget)?;
        if work.packets > budget.max_packets {
            return Err(Refusal::Budget);
        }
        // Do not trust planning alone. The original current stream validates the
        // complete mixed packet atomically, including exact proof consumption.
        call(&mut work, budget)?;
        native(c.packet(base.context, &packet))?;
        if c.tokenizer_offset() == input.raw.len() as u64 && c.pending_key().is_none() {
            break;
        }
    }
    if proof_index != input.proofs.len() {
        return Err(Refusal::ExtraProof { first_unused: proof_index });
    }
    call(&mut work, budget)?;
    native(c.seal_tokenizer(base.context))?;
    if let Some(bytes) = input.rendering {
        for part in bytes.chunks(65536) {
            call(&mut work, budget)?;
            native(c.rendering_part(base.context, c.rendering_offset(), part))?;
        }
    }
    call(&mut work, budget)?;
    let violation = native(c.finish())?;
    Ok(Finding { violation, key_root: c.key_set_root(), key_events: c.key_events(), work })
}
