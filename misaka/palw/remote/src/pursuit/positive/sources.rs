//! Panel-local authenticated source acquisition. The canonical coordinator owns
//! all request, delivery, payment and work counters; a cache never recreates it.
//! Data identity is separate from arithmetic checking and witness completion.

use kaspa_consensus_core::{
    Hash64,
    palw_positive_court_v2::{
        ErrorV2, MinerResultV2, SignatureV2,
        coordinator::PositiveCoordinatorV2,
        service::{CHUNK_BYTES, RequestEffectV2, ServicePolicyV2, WorkRequestV2, chunk_hash_v2},
    },
    palw_step_leg::{PalwStepOpeningV1, step_leg_max_opening_siblings_v1, step_opening_root_capped_v1},
};
use std::collections::BTreeMap;

/// Local DTO, with a fixed payload size. This is not a new native wire object.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceChunkV2 {
    pub bytes: Box<[u8; CHUNK_BYTES]>,
    pub opening: PalwStepOpeningV1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceCacheLimitsV2 {
    /// Includes payloads, opening siblings and a conservative per-entry overhead.
    pub max_retained_bytes: u64,
    /// Bounds provider attempts in one acquisition, including the final Miner.
    pub max_provider_attempts: usize,
}

/// A connector may represent a public provider or the Miner responder. It must
/// enforce its own transport/time limits and the supplied opening-depth bound
/// before allocating a response. It only serves existing source bytes.
pub trait PanelSourceProviderV2 {
    fn fetch_source(&mut self, index: u64, max_opening_siblings: usize) -> Result<Option<SourceChunkV2>, String>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceAcquisitionV2 {
    Cached,
    Delivered {
        provider: usize,
    },
    /// No valid source was obtained. This carries no Miner-default authority.
    Unavailable,
}

/// Transfer this cache between Panels, or export authenticated entries and
/// reimport them into a fresh cache. Neither action resets the Node's canonical
/// service budget. The coordinator must be reconstructed separately by its
/// authenticated service journal on restart; no cached receipt is a Node ACK.
pub struct PanelSourceCacheV2 {
    result: Hash64,
    policy: ServicePolicyV2,
    limits: SourceCacheLimitsV2,
    retained_bytes: u64,
    entries: BTreeMap<u64, SourceChunkV2>,
}
impl PanelSourceCacheV2 {
    /// `result` normally comes from admission. Every canonical operation below
    /// also checks its identity against the live admitted coordinator. Creating
    /// or populating this local cache alone never creates any service duty.
    pub fn new(result: &MinerResultV2, policy: ServicePolicyV2, limits: SourceCacheLimitsV2) -> Result<Self, ErrorV2> {
        if result.version != 2 || policy.version != 2 || policy.id() != result.service_policy || policy.case != result.claim {
            return Err(ErrorV2::Binding);
        }
        if policy.chunks == 0
            || policy.chunks > policy.max_unique_bytes / CHUNK_BYTES as u64
            || policy.retention_until < policy.deadline
            || policy.work_per_chunk == 0
            || limits.max_provider_attempts == 0
            || limits.max_retained_bytes < Self::entry_bound(&policy)?
        {
            return Err(ErrorV2::Budget);
        }
        Ok(Self { result: result.id(), policy, limits, retained_bytes: 0, entries: BTreeMap::new() })
    }

    fn entry_bound(policy: &ServicePolicyV2) -> Result<u64, ErrorV2> {
        (CHUNK_BYTES as u64)
            .checked_add(256)
            .and_then(|n| n.checked_add(64 * step_leg_max_opening_siblings_v1(policy.chunks) as u64))
            .ok_or(ErrorV2::Budget)
    }

    fn check_coordinator(&self, node: &PositiveCoordinatorV2) -> Result<(), ErrorV2> {
        if node.result().id() != self.result || node.result().service_policy != self.policy.id() {
            return Err(ErrorV2::Binding);
        }
        Ok(())
    }

    fn authenticate(&self, index: u64, chunk: &SourceChunkV2) -> Result<(), ErrorV2> {
        if index >= self.policy.chunks || chunk.opening.siblings.len() > step_leg_max_opening_siblings_v1(self.policy.chunks) {
            return Err(ErrorV2::Budget);
        }
        if chunk.opening.leaf_index != index
            || chunk.opening.leaf_hash != chunk_hash_v2(&self.policy.case, index, chunk.bytes.as_slice())
            || step_opening_root_capped_v1(self.policy.chunks, &chunk.opening, self.policy.chunks).map_err(|_| ErrorV2::Wire)?
                != self.policy.source
        {
            return Err(ErrorV2::Binding);
        }
        Ok(())
    }

    pub fn get(&self, index: u64) -> Option<&[u8; CHUNK_BYTES]> {
        self.entries.get(&index).map(|entry| entry.bytes.as_ref())
    }
    pub fn retained_bytes(&self) -> u64 {
        self.retained_bytes
    }
    pub fn cached_chunks(&self) -> u64 {
        self.entries.len() as u64
    }

    /// Import one bounded entry from a local cache transfer. All Merkle paths
    /// are checked afresh. A duplicate never changes quota or replaces data.
    /// This method deliberately does not mark canonical delivery complete.
    pub fn import_authenticated(&mut self, mut chunk: SourceChunkV2) -> Result<(), ErrorV2> {
        let index = chunk.opening.leaf_index;
        self.authenticate(index, &chunk)?;
        if self.entries.contains_key(&index) {
            return Ok(());
        }
        let retained = self.retained_bytes.checked_add(Self::entry_bound(&self.policy)?).ok_or(ErrorV2::Budget)?;
        if retained > self.limits.max_retained_bytes {
            return Err(ErrorV2::Budget);
        }
        // A local transfer may carry a short Vec with enormous spare capacity.
        // Retain only a fresh bounded allocation, never that hidden capacity.
        let mut siblings = Vec::with_capacity(chunk.opening.siblings.len());
        siblings.extend_from_slice(&chunk.opening.siblings);
        chunk.opening.siblings = siblings;
        self.entries.insert(index, chunk);
        self.retained_bytes = retained;
        Ok(())
    }

    /// Borrow entries in canonical order for bounded persistence or takeover.
    /// The receiver uses `import_authenticated`; there is no saved "verified"
    /// flag, last-request cursor or budget value to trust.
    pub fn authenticated_entries(&self) -> impl Iterator<Item = &SourceChunkV2> {
        self.entries.values()
    }

    /// Assemble every authenticated policy chunk in canonical order. The
    /// returned size includes the signed final chunk's padding; this adapter
    /// does not invent logical-length metadata or a new source container.
    ///
    /// Completeness and signed size bounds are checked before the first write.
    /// I/O failure propagates and may leave a partial destination, so persist to
    /// a temporary file and atomically publish only after success. The artifact
    /// adapter must still parse PALWTIR1 and match its registered artifact root.
    /// Source assembly does not imply successful computation or Node delivery.
    pub fn write_complete_source(&self, writer: &mut dyn std::io::Write) -> std::io::Result<u64> {
        let total = self
            .policy
            .chunks
            .checked_mul(CHUNK_BYTES as u64)
            .filter(|total| *total <= self.policy.max_unique_bytes)
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "source exceeds signed byte bound"))?;
        if self.policy.chunks == 0
            || self.entries.len() as u64 != self.policy.chunks
            || self.entries.keys().enumerate().any(|(index, actual)| index as u64 != *actual)
        {
            return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "authenticated source chunks are incomplete"));
        }
        for entry in self.entries.values() {
            writer.write_all(entry.bytes.as_slice())?;
        }
        Ok(total)
    }

    /// Plan the first contiguous missing run, never intermediate traces or
    /// witness-building work. Repeated plans are coalesced by the shared Node.
    pub fn next_missing_request(&self, first: u64, count: u64, max_chunks: u64) -> Result<Option<WorkRequestV2>, ErrorV2> {
        let end = first.checked_add(count).filter(|end| count > 0 && *end <= self.policy.chunks).ok_or(ErrorV2::Order)?;
        if max_chunks == 0 {
            return Err(ErrorV2::Budget);
        }
        let mut missing = first;
        for (&index, _) in self.entries.range(first..end) {
            if index != missing {
                break;
            }
            missing += 1;
        }
        if missing == end {
            return Ok(None);
        }
        let limit = missing.saturating_add(max_chunks).min(end);
        let stop = self.entries.range(missing..limit).next().map_or(limit, |(&index, _)| index);
        Ok(Some(WorkRequestV2::Source { first: missing, count: stop - missing }))
    }

    /// The signature is over `node.request_id(request)` using the common source
    /// request context. There is exactly one canonical ledger across all Panels.
    pub fn request(
        &self,
        node: &mut PositiveCoordinatorV2,
        panel: Hash64,
        request: WorkRequestV2,
        signature: &SignatureV2,
        now: u64,
    ) -> Result<RequestEffectV2, ErrorV2> {
        self.check_coordinator(node)?;
        node.request_source(panel, request, signature, now)
    }

    /// Deliver data already held by a warm/replacement Panel directly to the
    /// canonical Node. This authenticates real bytes; no Panel receipt is used.
    pub fn deliver_cached(&self, node: &mut PositiveCoordinatorV2, index: u64, now: u64) -> Result<(), ErrorV2> {
        self.check_coordinator(node)?;
        let chunk = self.entries.get(&index).ok_or(ErrorV2::Incomplete)?;
        node.respond_source(index, chunk.bytes.as_slice(), &chunk.opening, now)
    }

    /// Try providers in caller order (normally public providers, then Miner).
    /// Requests must already be accepted by `request`. Invalid delivered bytes
    /// are presented to the canonical Node and spend its shared transmission /
    /// work allowance before fallback. A storage-full cache refuses before any
    /// fetch. The callback never asks a Miner to generate a trace or witness.
    /// `clock` returns the current admitted DAA before and after blocking I/O.
    /// A native connector must also refresh its admitted branch/view after I/O;
    /// a stale admission must not be submitted blindly to another branch.
    pub fn acquire_one(
        &mut self,
        node: &mut PositiveCoordinatorV2,
        index: u64,
        providers: &mut [&mut dyn PanelSourceProviderV2],
        clock: &mut dyn FnMut() -> u64,
    ) -> Result<SourceAcquisitionV2, ErrorV2> {
        self.check_coordinator(node)?;
        if clock() >= self.policy.deadline {
            return Err(ErrorV2::Deadline);
        }
        if index >= self.policy.chunks {
            return Err(ErrorV2::Order);
        }
        if self.entries.contains_key(&index) {
            return Ok(SourceAcquisitionV2::Cached);
        }
        if providers.is_empty() || providers.len() > self.limits.max_provider_attempts {
            return Err(ErrorV2::Budget);
        }
        if self.retained_bytes.checked_add(Self::entry_bound(&self.policy)?).ok_or(ErrorV2::Budget)? > self.limits.max_retained_bytes {
            return Err(ErrorV2::Budget);
        }
        for (provider, source) in providers.iter_mut().enumerate() {
            if clock() >= self.policy.deadline {
                return Err(ErrorV2::Deadline);
            }
            let response = source.fetch_source(index, step_leg_max_opening_siblings_v1(self.policy.chunks));
            let now = clock();
            if now >= self.policy.deadline {
                return Err(ErrorV2::Deadline);
            }
            let Ok(Some(chunk)) = response else {
                continue;
            };
            // This envelope exceeds the connector's explicit local bound. It
            // cannot enter the canonical parser or consume a new service duty;
            // skip it within the bounded provider list and try the next source.
            if chunk.opening.siblings.len() > step_leg_max_opening_siblings_v1(self.policy.chunks) {
                continue;
            }
            match node.respond_source(index, chunk.bytes.as_slice(), &chunk.opening, now) {
                Ok(()) => {
                    self.import_authenticated(chunk)?;
                    return Ok(SourceAcquisitionV2::Delivered { provider });
                }
                // These errors mean this provider supplied invalid material;
                // canonical work/byte counters are retained across fallback.
                Err(ErrorV2::Binding | ErrorV2::Wire) => continue,
                Err(error) => return Err(error),
            }
        }
        Ok(SourceAcquisitionV2::Unavailable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_step_leg::PalwStepMerkleTreeV1;

    fn fixture() -> (PanelSourceCacheV2, Vec<SourceChunkV2>) {
        let case = Hash64::from_u64_word(77);
        let payloads: Vec<_> = (0..4u8).map(|byte| Box::new([byte; CHUNK_BYTES])).collect();
        let hashes: Vec<_> =
            payloads.iter().enumerate().map(|(index, bytes)| chunk_hash_v2(&case, index as u64, bytes.as_slice())).collect();
        let tree = PalwStepMerkleTreeV1::build_capped_v1(&hashes, 4).unwrap();
        let policy = ServicePolicyV2 {
            version: 2,
            case,
            source: tree.root(),
            chunks: 4,
            max_rounds: 4,
            max_unique_bytes: 4 * CHUNK_BYTES as u64,
            max_transmitted_bytes: 8 * (CHUNK_BYTES as u64 + 204),
            work_per_chunk: 10,
            max_work: 80,
            retention_until: 110,
            deadline: 100,
            price_per_chunk: 7,
            hold: 28,
        };
        let cache = PanelSourceCacheV2 {
            result: Hash64::from_u64_word(78),
            policy,
            limits: SourceCacheLimitsV2 { max_retained_bytes: 1 << 20, max_provider_attempts: 2 },
            retained_bytes: 0,
            entries: BTreeMap::new(),
        };
        let chunks = payloads
            .into_iter()
            .enumerate()
            .map(|(index, bytes)| SourceChunkV2 {
                bytes,
                opening: PalwStepOpeningV1 {
                    leaf_index: index as u64,
                    leaf_hash: hashes[index],
                    siblings: tree.path_v1(index).unwrap(),
                },
            })
            .collect();
        (cache, chunks)
    }

    #[test]
    fn authenticated_takeover_reuses_sources_and_requests_only_missing_ranges() {
        let (mut cache, chunks) = fixture();
        cache.import_authenticated(chunks[0].clone()).unwrap();
        cache.import_authenticated(chunks[2].clone()).unwrap();
        assert_eq!(cache.next_missing_request(0, 4, 4), Ok(Some(WorkRequestV2::Source { first: 1, count: 1 })));
        let (mut replacement, _) = fixture();
        for chunk in cache.authenticated_entries() {
            replacement.import_authenticated(chunk.clone()).unwrap();
        }
        assert_eq!(replacement.get(0), Some(chunks[0].bytes.as_ref()));
        assert_eq!(replacement.retained_bytes(), cache.retained_bytes());
        let retained = replacement.retained_bytes();
        replacement.import_authenticated(chunks[0].clone()).unwrap();
        assert_eq!(replacement.retained_bytes(), retained);
        replacement.import_authenticated(chunks[1].clone()).unwrap();
        assert_eq!(replacement.next_missing_request(0, 4, 1), Ok(Some(WorkRequestV2::Source { first: 3, count: 1 })));
        replacement.import_authenticated(chunks[3].clone()).unwrap();
        assert_eq!(replacement.next_missing_request(0, 4, 4), Ok(None));
    }

    #[test]
    fn tampered_source_or_opening_is_never_cached() {
        let (mut cache, chunks) = fixture();
        let mut altered = chunks[0].clone();
        altered.bytes[23] ^= 1;
        assert_eq!(cache.import_authenticated(altered), Err(ErrorV2::Binding));
        let mut swapped = chunks[0].clone();
        swapped.opening.leaf_index = 1;
        assert_eq!(cache.import_authenticated(swapped), Err(ErrorV2::Binding));
        let mut path = chunks[0].clone();
        path.opening.siblings[0] = Hash64::from_u64_word(999);
        assert_eq!(cache.import_authenticated(path), Err(ErrorV2::Binding));
        assert_eq!(cache.cached_chunks(), 0);
        assert_eq!(cache.retained_bytes(), 0);
    }

    #[test]
    fn cross_case_transfer_and_excessive_opening_are_rejected() {
        let (mut cache, chunks) = fixture();
        cache.policy.case = Hash64::from_u64_word(88);
        assert_eq!(cache.import_authenticated(chunks[0].clone()), Err(ErrorV2::Binding));
        let (mut cache, _) = fixture();
        let mut chunk = chunks[0].clone();
        chunk.opening.siblings.push(Hash64::default());
        assert_eq!(cache.import_authenticated(chunk), Err(ErrorV2::Budget));
        assert_eq!(cache.cached_chunks(), 0);
    }

    #[test]
    fn retention_cap_is_atomic_and_duplicates_do_not_consume_it() {
        let (mut cache, chunks) = fixture();
        cache.limits.max_retained_bytes = PanelSourceCacheV2::entry_bound(&cache.policy).unwrap();
        cache.import_authenticated(chunks[0].clone()).unwrap();
        cache.import_authenticated(chunks[0].clone()).unwrap();
        assert_eq!(cache.import_authenticated(chunks[1].clone()), Err(ErrorV2::Budget));
        assert_eq!(cache.cached_chunks(), 1);
        assert!(cache.get(1).is_none());
    }

    #[test]
    fn source_assembly_preflights_completeness_before_writing() {
        let (mut cache, chunks) = fixture();
        cache.import_authenticated(chunks[0].clone()).unwrap();
        cache.import_authenticated(chunks[2].clone()).unwrap();
        let mut output = Vec::new();
        let error = cache.write_complete_source(&mut output).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::UnexpectedEof);
        assert!(output.is_empty());
    }

    #[test]
    fn source_assembly_writes_all_chunks_in_canonical_order() {
        let (mut cache, chunks) = fixture();
        for chunk in chunks.iter().rev() {
            cache.import_authenticated(chunk.clone()).unwrap();
        }
        let mut output = Vec::new();
        assert_eq!(cache.write_complete_source(&mut output).unwrap(), 4 * CHUNK_BYTES as u64);
        assert_eq!(output.len(), 4 * CHUNK_BYTES);
        for (index, bytes) in output.chunks_exact(CHUNK_BYTES).enumerate() {
            assert_eq!(bytes, chunks[index].bytes.as_slice());
        }
    }

    #[test]
    fn source_assembly_propagates_writer_failure_without_success() {
        struct FailAfter {
            remaining: usize,
        }
        impl std::io::Write for FailAfter {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                if self.remaining == 0 {
                    return Err(std::io::Error::new(std::io::ErrorKind::BrokenPipe, "test destination failed"));
                }
                let written = bytes.len().min(self.remaining);
                self.remaining -= written;
                Ok(written)
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let (mut cache, chunks) = fixture();
        for chunk in chunks {
            cache.import_authenticated(chunk).unwrap();
        }
        let mut writer = FailAfter { remaining: CHUNK_BYTES + 10 };
        assert_eq!(cache.write_complete_source(&mut writer).unwrap_err().kind(), std::io::ErrorKind::BrokenPipe);
        assert_eq!(writer.remaining, 0);
    }

    #[test]
    fn imported_opening_discards_hidden_spare_capacity() {
        let (mut cache, chunks) = fixture();
        let mut chunk = chunks[0].clone();
        chunk.opening.siblings.reserve_exact(1 << 18);
        assert!(chunk.opening.siblings.capacity() > 1 << 18);
        cache.import_authenticated(chunk).unwrap();
        let retained = cache.entries.get(&0).unwrap();
        assert_eq!(retained.opening.siblings.len(), 2);
        assert_eq!(retained.opening.siblings.capacity(), 2);
        assert_eq!(cache.retained_bytes(), PanelSourceCacheV2::entry_bound(&cache.policy).unwrap());
    }

    #[test]
    fn huge_or_invalid_ranges_are_bounded_without_per_missing_chunk_loops() {
        let (mut cache, _) = fixture();
        assert_eq!(cache.next_missing_request(u64::MAX, 2, 1), Err(ErrorV2::Order));
        assert_eq!(cache.next_missing_request(0, 0, 1), Err(ErrorV2::Order));
        assert_eq!(cache.next_missing_request(0, 4, 0), Err(ErrorV2::Budget));
        cache.policy.chunks = u64::MAX / CHUNK_BYTES as u64;
        assert_eq!(
            cache.next_missing_request(0, cache.policy.chunks, u64::MAX),
            Ok(Some(WorkRequestV2::Source { first: 0, count: cache.policy.chunks }))
        );
    }
}
