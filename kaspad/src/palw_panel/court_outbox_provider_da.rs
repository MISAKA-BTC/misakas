//! Local provenance for an ordinary DA answer built from a provider response.
//! The packet stays owned by the normal outbox. Only fixed-size metadata and
//! its reservation are retained here.
use super::{CourtMoveKeyV1, CourtMoveV1, Hash64, PalwConsensusObjectV2};
use kaspa_consensus_core::{
    palw_da_rcore_v1::{PalwDaAnswerV1, PalwDaUnitV1},
    palw_legacy_answer_chunks_v2::PalwLegacyAnswerManifestPinV2,
    palw_native_resource_v1::NativeHeapV1,
    palw_producer_v2::PalwDisclosureDutyV1,
    palw_step_refute::PalwTraceEventDisclosureV1,
};
use kaspa_core::memory_budget::{MemoryClass, MemoryLease, global_memory_budget};
use misaka_palw_remote::evidence::ClaimRoots;

const MAX_PROVIDER_DA_ANSWERS: usize = 64;

pub(in crate::palw_panel) struct ProviderDaBuiltAnswerV1 {
    pub object: PalwConsensusObjectV2,
    pub manifest: Option<PalwLegacyAnswerManifestPinV2>,
}

pub(in crate::palw_panel) struct ProviderDaStampV1 {
    pub duty: PalwDisclosureDutyV1,
    pub roots: ClaimRoots,
    pub manifest: Option<PalwLegacyAnswerManifestPinV2>,
}

impl ProviderDaStampV1 {
    pub(in crate::palw_panel) fn is_flat(object: &PalwConsensusObjectV2) -> bool {
        matches!(
            object,
            PalwConsensusObjectV2::MaterialDisclosedV2 { answer: PalwDaAnswerV1::Event(PalwTraceEventDisclosureV1::Flat { .. }), .. }
        )
    }

    fn matches_packet(&self, object: &PalwConsensusObjectV2) -> bool {
        use PalwConsensusObjectV2 as O;
        let (claim, unit, discloser) = match object {
            O::MaterialDisclosedV2 { claim, unit, discloser, .. } => (*claim, *unit, *discloser),
            O::LegacyHeldAnsweredV2 { answer } => (answer.claim, PalwDaUnitV1::LegacyHeldV2(answer.unit), answer.discloser),
            O::LegacyHeldAnswerDeclaredV2 { declaration } => {
                (declaration.claim, PalwDaUnitV1::LegacyHeldV2(declaration.unit), declaration.discloser)
            }
            O::LegacyHeldAnswerChunkV2 { chunk } => (chunk.claim, PalwDaUnitV1::LegacyHeldV2(chunk.unit), chunk.discloser),
            _ => return false,
        };
        (claim, unit, discloser) == (self.duty.claim_id, self.duty.unit, self.duty.discloser)
            && self.manifest.is_some() == matches!(object, O::LegacyHeldAnswerChunkV2 { .. })
    }
}

struct Entry {
    key: CourtMoveKeyV1,
    stamp: ProviderDaStampV1,
    packet: Hash64,
    _payload: MemoryLease,
}

#[derive(Default)]
pub(super) struct ProviderDaGuardsV1 {
    entries: Vec<Entry>,
    frame: Option<MemoryLease>,
    in_flight: bool,
}

impl ProviderDaGuardsV1 {
    /// A queue key is a debounce hash, not packet identity.
    fn digest(object: &PalwConsensusObjectV2) -> Result<Hash64, String> {
        struct Writer(blake2b_simd::State);
        impl std::io::Write for Writer {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.0.update(bytes);
                Ok(bytes.len())
            }

            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut writer = Writer(blake2b_simd::Params::new().hash_length(64).key(b"misaka-node/provider-da-queue/v1").to_state());
        borsh::BorshSerialize::serialize(object, &mut writer).map_err(|e| e.to_string())?;
        Ok(Hash64::from_bytes(*writer.0.finalize().as_array()))
    }

    pub(super) fn insert(
        &mut self,
        key: CourtMoveKeyV1,
        stamp: ProviderDaStampV1,
        object: &PalwConsensusObjectV2,
    ) -> Result<(), String> {
        if self.entries.len() >= MAX_PROVIDER_DA_ANSWERS || self.entries.iter().any(|entry| entry.key == key) {
            return Err("provider DA outbox metadata is full or the key is in use".into());
        }
        if !stamp.matches_packet(object) {
            return Err("provider DA packet differs from its duty".into());
        }
        let packet = Self::digest(object)?;
        if self.frame.is_none() {
            let bytes = (MAX_PROVIDER_DA_ANSWERS * std::mem::size_of::<Entry>()) as u64;
            let frame = global_memory_budget().try_reserve(MemoryClass::Transport, bytes).map_err(|e| e.to_string())?;
            self.entries.try_reserve_exact(MAX_PROVIDER_DA_ANSWERS).map_err(|e| e.to_string())?;
            frame.materialized();
            self.frame = Some(frame);
        }
        let coverage = object.native_heap_v1("provider DA outbox payload");
        let bytes = coverage.bytes().ok_or_else(|| coverage.to_string())?;
        let payload = global_memory_budget().try_reserve_retained(MemoryClass::Transport, bytes).map_err(|e| e.to_string())?;
        self.entries.push(Entry { key, stamp, packet, _payload: payload });
        Ok(())
    }

    pub(super) fn stamp(&self, key: CourtMoveKeyV1, object: &PalwConsensusObjectV2) -> Result<Option<&ProviderDaStampV1>, String> {
        let Some(entry) = self.entries.iter().find(|entry| entry.key == key) else { return Ok(None) };
        if Self::digest(object)? != entry.packet {
            return Err("provider DA queue key refers to a different packet".into());
        }
        Ok(Some(&entry.stamp))
    }

    pub(super) fn start_flush(&mut self) {
        self.in_flight = true;
    }

    pub(super) fn finish_flush(&mut self, pending: &[CourtMoveV1]) {
        self.in_flight = false;
        self.prune(pending);
    }

    pub(super) fn prune(&mut self, pending: &[CourtMoveV1]) {
        if !self.in_flight {
            self.entries.retain(|entry| pending.iter().any(|(s, r, b, _)| (*s, *r, *b) == entry.key));
        }
    }

    #[cfg(test)]
    pub(super) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::{
        palw_legacy_answer_chunks_v2::{PalwLegacyAnswerChunkV2, PalwLegacyAnswerDeclarationV2},
        palw_legacy_held_da_v2::PalwLegacyHeldUnitV2,
        palw_producer_v2::PalwDisclosureRoleV1,
        palw_state_v2::PalwBondKeyV2,
        tx::TransactionOutpoint,
    };

    fn fixture() -> (ProviderDaStampV1, PalwLegacyAnswerChunkV2) {
        let hash = Hash64::from_u64_word;
        let discloser = PalwBondKeyV2(TransactionOutpoint::new(hash(1), 0));
        let chunk = PalwLegacyAnswerChunkV2 {
            claim: hash(2),
            unit: PalwLegacyHeldUnitV2::PromptIds { chunk: 0 },
            discloser,
            answer_digest: hash(3),
            index: 0,
            bytes: vec![7],
        };
        let roots = ClaimRoots {
            network_domain: hash(4),
            trace_root: hash(5),
            output_root: hash(6),
            execution_root: hash(7),
            trace_chunk_count: 1,
            retention_deadline: 120,
        };
        let duty = PalwDisclosureDutyV1 {
            claim_id: chunk.claim,
            accepted_block: hash(8),
            class_id: hash(9),
            artifact_root: hash(10),
            executor_bond: discloser,
            discloser,
            role: PalwDisclosureRoleV1::Producer,
            signer_rank: 0,
            unit: PalwDaUnitV1::LegacyHeldV2(chunk.unit),
            deadline_daa: 100,
            disclose_window_daa: 20,
            in_run_rows: 1,
            trace_root: roots.trace_root,
            execution_root: roots.execution_root,
            free_prompt: false,
            job_identity: hash(11),
        };
        let declaration = PalwLegacyAnswerDeclarationV2 {
            version: 2,
            claim: chunk.claim,
            unit: chunk.unit,
            discloser: chunk.discloser,
            part_bytes: 1,
            total_bytes: 1,
            chunk_digests: vec![kaspa_consensus_core::palw_legacy_answer_chunks_v2::palw_legacy_answer_chunk_digest_v2(
                0,
                &chunk.bytes,
            )],
            answer_digest: chunk.answer_digest,
            signature: vec![],
        };
        let manifest = Some(PalwLegacyAnswerManifestPinV2::new(&declaration, 99).unwrap());
        (ProviderDaStampV1 { duty, roots, manifest }, chunk)
    }

    #[test]
    fn provider_da_packet_identity_survives_flight_and_retry() {
        let (stamp, mut chunk) = fixture();
        let object = PalwConsensusObjectV2::LegacyHeldAnswerChunkV2 { chunk: Box::new(chunk.clone()) };
        let key = (stamp.duty.claim_id, 1, true);
        let mut outbox = super::super::CourtOutboxV1::default();
        outbox.try_push_provider_da(key, object, 100, stamp).unwrap();
        let batch = outbox.take_priority();
        assert!(outbox.provider_da_stamp(key, &batch[0].3).unwrap().is_some());

        chunk.bytes[0] ^= 1;
        let different = PalwConsensusObjectV2::LegacyHeldAnswerChunkV2 { chunk: Box::new(chunk) };
        assert!(outbox.provider_da_stamp(key, &different).is_err());
        outbox.finish_fixture_flush(batch);
        assert!(outbox.provider_da_stamp(key, &outbox.pending()[0].3).unwrap().is_some());

        drop(outbox.take_priority());
        outbox.finish_fixture_flush(vec![]);
        assert!(outbox.provider_da_stamp(key, &different).unwrap().is_none());
    }
}
