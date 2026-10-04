//! **Stage B — evidence the miner does not have to serve (transport and the panel's fetch).**
//!
//! The canonical manifest, its ids and its checks live in consensus-core ([`kaspa_consensus_core::palw_evidence_v1`]) because the provider
//! challenge court verifies an opening against the very same rules; they are re-exported here. What is here is what a *client* adds: the
//! [`ChunkProvider`] seam, [`fetch_material_any`] (any provider, every byte checked against a manifest that agrees with the claim), and a
//! directory provider ([`fs`]) — the dumbest transport that exists, which is the point: nothing a provider says is trusted.

use crate::{finish, keyed, put_len};
pub use kaspa_consensus_core::palw_evidence_court_v1::{
    ChallengeOutcome, ProviderChallengeV1, Responsible, challenge_outcome_v1, evidence_responsibility_v1,
};
pub use kaspa_consensus_core::palw_evidence_v1::*;
use kaspa_hashes::Hash64;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct ProviderError(pub String);

/// One byte-serving provider. `Err` is a local failure (timeout, refusal, not found) and is NEVER evidence of anything beyond "this attempt
/// failed".
pub trait ChunkProvider {
    fn provider_id(&self) -> &str;
    fn fetch_chunk(&self, manifest_id: Hash64, index: u32) -> Result<Vec<u8>, ProviderError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FetchFailureKind {
    Unavailable(String),
    /// The provider answered with bytes that do not match the manifest — recorded, never accepted.
    BadBytes(EvidenceError),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderFailure {
    pub provider: String,
    pub index: u32,
    pub kind: FetchFailureKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FetchReport {
    /// Who served each accepted chunk, in chunk order.
    pub served_by: Vec<String>,
    /// Every failed attempt, in the order tried — the raw material of a future provider challenge, and of nothing else.
    pub failures: Vec<ProviderFailure>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FetchFailure {
    #[error("the manifest is not admissible: {0}")]
    Manifest(EvidenceError),
    #[error("no provider produced a chunk that matches the manifest for chunk {index}: {failures:?}")]
    ChunkUnavailable { index: u32, failures: Vec<ProviderFailure> },
}

/// The order providers are tried for a chunk: deterministic per (seed, chunk, provider), so two Panel seats do not all hammer the first one
/// and a replay tries the same order.
fn order_for(seed: Hash64, index: u32, providers: &[&dyn ChunkProvider]) -> Vec<usize> {
    let mut keyed_order: Vec<(Hash64, usize)> = providers
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let mut s = keyed(DOMAIN_FETCH_ORDER);
            s.update(seed.as_bytes().as_slice());
            s.update(&index.to_le_bytes());
            put_len(&mut s, p.provider_id().as_bytes());
            (finish(s), i)
        })
        .collect();
    keyed_order.sort();
    keyed_order.into_iter().map(|(_, i)| i).collect()
}

/// Fetch one chunk from ANY provider: the first whose bytes verify against the manifest.
pub fn fetch_chunk_any(
    manifest: &EvidenceManifestV1,
    index: u32,
    providers: &[&dyn ChunkProvider],
    order_seed: Hash64,
) -> Result<(Vec<u8>, String, Vec<ProviderFailure>), FetchFailure> {
    let manifest_id = manifest_id_v1(manifest);
    let mut failures = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for i in order_for(order_seed, index, providers) {
        let provider = providers[i];
        if !seen.insert(provider.provider_id().to_string()) {
            continue;
        }
        match provider.fetch_chunk(manifest_id, index) {
            Ok(bytes) => match manifest.verify_chunk(index, &bytes) {
                Ok(()) => return Ok((bytes, provider.provider_id().to_string(), failures)),
                Err(e) => failures.push(ProviderFailure { provider: provider.provider_id().to_string(), index, kind: FetchFailureKind::BadBytes(e) }),
            },
            Err(e) => failures.push(ProviderFailure { provider: provider.provider_id().to_string(), index, kind: FetchFailureKind::Unavailable(e.0) }),
        }
    }
    Err(FetchFailure::ChunkUnavailable { index, failures })
}

/// Fetch the whole material: the manifest is validated, bound to the claim and to the network first; then every chunk from any provider.
pub fn fetch_material_any(
    manifest: &EvidenceManifestV1,
    claim: &ClaimRoots,
    limits: &ManifestLimits,
    providers: &[&dyn ChunkProvider],
    order_seed: Hash64,
) -> Result<(Vec<Vec<u8>>, FetchReport), FetchFailure> {
    manifest.validate_shape(limits).map_err(FetchFailure::Manifest)?;
    manifest.verify_claim_binding(claim).map_err(FetchFailure::Manifest)?;
    let mut material = Vec::with_capacity(manifest.chunks.len());
    let mut report = FetchReport { served_by: Vec::new(), failures: Vec::new() };
    for chunk in &manifest.chunks {
        let (bytes, provider, mut failures) = fetch_chunk_any(manifest, chunk.index, providers, order_seed)?;
        material.push(bytes);
        report.served_by.push(provider);
        report.failures.append(&mut failures);
    }
    Ok((material, report))
}

// ---------------------------------------------------------------------------------------------------------------------------------
// A concrete provider: a directory. The transport is deliberately the dumbest one that exists — a content-addressed tree of files — because
// everything that matters is verified against the manifest and the claim, so any carrier (rsync, a torrent's output directory, an object-store
// mount, a pinned IPFS gateway directory) can serve it and none of them is trusted.
//
//   <root>/claims/<claim id hex>.manifest          borsh(EvidenceManifestV1)         — written LAST, so a reader never sees a manifest
//   <root>/chunks/<manifest id hex>/<index>.chunk  the raw chunk bytes                  without every chunk it names
// ---------------------------------------------------------------------------------------------------------------------------------

pub mod fs {
    use super::*;
    use std::path::{Path, PathBuf};

    pub fn manifest_path(root: &Path, claim_hex: &str) -> PathBuf {
        root.join("claims").join(format!("{claim_hex}.manifest"))
    }

    pub fn chunk_path(root: &Path, manifest_id: Hash64, index: u32) -> PathBuf {
        root.join("chunks").join(manifest_id.to_string()).join(format!("{index}.chunk"))
    }

    /// Split material into chunks of at most `chunk_bytes` (at least one chunk, even for empty material).
    pub fn chunk_material(material: &[u8], chunk_bytes: usize) -> Vec<Vec<u8>> {
        let size = chunk_bytes.max(1);
        if material.is_empty() {
            return vec![Vec::new()];
        }
        material.chunks(size).map(<[u8]>::to_vec).collect()
    }

    /// Place a claim's material in a provider directory (the miner's last act before it may switch off). Write order is the safety:
    /// chunks first, each `.partial` then renamed, the manifest last.
    pub fn publish(root: &Path, claim_hex: &str, manifest: &EvidenceManifestV1, chunks: &[Vec<u8>]) -> std::io::Result<()> {
        let id = manifest_id_v1(manifest);
        let dir = root.join("chunks").join(id.to_string());
        std::fs::create_dir_all(&dir)?;
        std::fs::create_dir_all(root.join("claims"))?;
        for (i, bytes) in chunks.iter().enumerate() {
            let path = dir.join(format!("{i}.chunk"));
            let tmp = path.with_extension("partial");
            std::fs::write(&tmp, bytes)?;
            std::fs::rename(&tmp, &path)?;
        }
        let path = manifest_path(root, claim_hex);
        let tmp = path.with_extension("partial");
        std::fs::write(&tmp, borsh::to_vec(manifest).expect("borsh-serializable"))?;
        std::fs::rename(&tmp, &path)
    }

    /// One directory as a [`ChunkProvider`].
    pub struct FsProvider {
        pub id: String,
        pub root: PathBuf,
    }

    impl FsProvider {
        pub fn new(root: impl Into<PathBuf>) -> Self {
            let root = root.into();
            Self { id: root.display().to_string(), root }
        }

        /// The manifest this provider holds for a claim, if any. NOT trusted: the caller validates it against the claim.
        pub fn manifest_for(&self, claim_hex: &str) -> Option<EvidenceManifestV1> {
            let bytes = std::fs::read(manifest_path(&self.root, claim_hex)).ok()?;
            let mut slice = bytes.as_slice();
            let manifest = <EvidenceManifestV1 as borsh::BorshDeserialize>::deserialize(&mut slice).ok()?;
            slice.is_empty().then_some(manifest)
        }
    }

    impl ChunkProvider for FsProvider {
        fn provider_id(&self) -> &str {
            &self.id
        }
        fn fetch_chunk(&self, manifest_id: Hash64, index: u32) -> Result<Vec<u8>, ProviderError> {
            std::fs::read(chunk_path(&self.root, manifest_id, index)).map_err(|e| ProviderError(e.to_string()))
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
    pub enum ClaimFetchError {
        #[error("no provider holds a manifest for this claim that agrees with the claim's own roots (offered {offered}, refused: {refusals:?})")]
        NoAdmissibleManifest { offered: usize, refusals: Vec<String> },
        #[error("{0}")]
        Fetch(FetchFailure),
    }

    /// **The panel's material from ANY provider directory.** Each provider's manifest is judged against the CLAIM (shape, network, the three
    /// roots, chunk count, retention) and the first admissible one wins; then every chunk comes from any provider, verified against that
    /// manifest. The reassembled material is returned to the caller, which hands it to the same root verification and re-execution a peer's
    /// copy goes through — this adds a source, it does not replace a check.
    pub fn fetch_claim_material(
        roots: &[PathBuf],
        claim_hex: &str,
        claim: &ClaimRoots,
        limits: &ManifestLimits,
        order_seed: Hash64,
    ) -> Result<(Vec<u8>, FetchReport), ClaimFetchError> {
        let providers: Vec<FsProvider> = roots.iter().map(FsProvider::new).collect();
        let mut refusals = Vec::new();
        let mut offered = 0usize;
        let mut chosen = None;
        for p in &providers {
            let Some(m) = p.manifest_for(claim_hex) else { continue };
            offered += 1;
            match m.validate_shape(limits).and_then(|()| m.verify_claim_binding(claim)) {
                Ok(()) => {
                    chosen = Some(m);
                    break;
                }
                Err(e) => refusals.push(format!("{}: {e}", p.id)),
            }
        }
        let Some(manifest) = chosen else {
            return Err(ClaimFetchError::NoAdmissibleManifest { offered, refusals });
        };
        let refs: Vec<&dyn ChunkProvider> = providers.iter().map(|p| p as &dyn ChunkProvider).collect();
        let (chunks, report) = fetch_material_any(&manifest, claim, limits, &refs, order_seed).map_err(ClaimFetchError::Fetch)?;
        Ok((chunks.concat(), report))
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::tx::TransactionOutpoint;
    use std::cell::RefCell;
    use std::collections::BTreeMap;


    fn h(n: u8) -> Hash64 {
        Hash64::from_bytes([n; 64])
    }
    fn bond() -> TransactionOutpoint {
        TransactionOutpoint::new(h(0xB0), 1)
    }
    fn chunks() -> Vec<Vec<u8>> {
        vec![vec![1u8; 100], vec![2u8; 50], vec![3u8; 7]]
    }
    fn manifest() -> EvidenceManifestV1 {
        EvidenceManifestV1::build(h(9), &bond(), &[5u8; 32], h(1), h(2), h(3), 3, 10_000, &chunks())
    }
    fn claim() -> ClaimRoots {
        ClaimRoots { network_domain: h(9), trace_root: h(1), output_root: h(2), execution_root: h(3), trace_chunk_count: 3, retention_deadline: 9_000 }
    }

    struct Prov {
        id: &'static str,
        chunks: BTreeMap<u32, Vec<u8>>,
        down: bool,
        asked: RefCell<Vec<u32>>,
    }
    impl Prov {
        fn honest(id: &'static str) -> Self {
            Self { id, chunks: chunks().into_iter().enumerate().map(|(i, c)| (i as u32, c)).collect(), down: false, asked: Default::default() }
        }
    }
    impl ChunkProvider for Prov {
        fn provider_id(&self) -> &str {
            self.id
        }
        fn fetch_chunk(&self, _: Hash64, index: u32) -> Result<Vec<u8>, ProviderError> {
            self.asked.borrow_mut().push(index);
            if self.down {
                return Err(ProviderError("connection refused".into()));
            }
            self.chunks.get(&index).cloned().ok_or(ProviderError("not found".into()))
        }
    }

    #[test]
    fn the_material_is_fetched_from_any_provider_and_a_liar_or_a_dead_one_is_recorded_not_believed() {
        // Every chunk must be fetched from the one honest provider, so the others are all tried (in some order) for every chunk.
        let mut liar = Prov::honest("liar");
        for bytes in liar.chunks.values_mut() {
            bytes[0] ^= 0xFF; // right length, wrong bytes
        }
        let mut dead = Prov::honest("dead");
        dead.down = true;
        let mut partial = Prov::honest("partial");
        partial.chunks.clear();
        let good = Prov::honest("good");
        let providers: Vec<&dyn ChunkProvider> = vec![&liar, &dead, &partial, &good];
        let (material, report) = fetch_material_any(&manifest(), &claim(), &ManifestLimits::default(), &providers, h(0x5E)).unwrap();
        assert_eq!(material, chunks(), "the bytes are the executor's, whoever served them");
        assert_eq!(report.served_by.len(), 3);
        assert!(report.failures.iter().any(|f| f.provider == "liar" && matches!(f.kind, FetchFailureKind::BadBytes(_))), "a liar is recorded");
        assert!(report.failures.iter().any(|f| f.provider == "dead" && matches!(f.kind, FetchFailureKind::Unavailable(_))), "a dead one is recorded");
        assert!(report.served_by.iter().all(|p| p == "good"), "only verified bytes are ever accepted");
        // The order is deterministic in (seed, chunk, provider): the same seed tries the same order, so a replay is reproducible.
        let again = fetch_material_any(&manifest(), &claim(), &ManifestLimits::default(), &providers, h(0x5E)).unwrap();
        assert_eq!(again.1.served_by, report.served_by);
    }

    #[test]
    fn when_no_provider_has_a_chunk_the_failure_names_each_provider_and_nothing_is_accepted() {
        let mut a = Prov::honest("a");
        let mut b = Prov::honest("b");
        a.chunks.remove(&1);
        b.chunks.get_mut(&1).unwrap().push(0); // wrong length
        let providers: Vec<&dyn ChunkProvider> = vec![&a, &b];
        match fetch_material_any(&manifest(), &claim(), &ManifestLimits::default(), &providers, h(1)) {
            Err(FetchFailure::ChunkUnavailable { index: 1, failures }) => assert_eq!(failures.len(), 2),
            other => panic!("{other:?}"),
        }
        let none: Vec<&dyn ChunkProvider> = vec![];
        assert!(matches!(fetch_material_any(&manifest(), &claim(), &ManifestLimits::default(), &none, h(1)), Err(FetchFailure::ChunkUnavailable { index: 0, .. })));
    }

    #[test]
    fn a_manifest_for_another_execution_is_refused_before_a_single_byte_is_asked_for() {
        let good = Prov::honest("good");
        let providers: Vec<&dyn ChunkProvider> = vec![&good];
        let mut c = claim();
        c.output_root = h(0x66);
        let r = fetch_material_any(&manifest(), &c, &ManifestLimits::default(), &providers, h(1));
        assert_eq!(r, Err(FetchFailure::Manifest(EvidenceError::RootMismatch("output"))));
        assert!(good.asked.borrow().is_empty(), "no provider was asked");
    }


    #[test]
    fn a_provider_directory_serves_a_claims_material_to_the_panel_and_a_forged_directory_is_refused() {
        let tmp = std::env::temp_dir().join(format!("rfc9-evidence-{}", std::process::id()));
        let (honest, forger, empty) = (tmp.join("honest"), tmp.join("forger"), tmp.join("empty"));
        std::fs::create_dir_all(&empty).unwrap();
        let material: Vec<u8> = (0..200u32).map(|i| (i % 251) as u8).collect();
        let chunks = fs::chunk_material(&material, 64);
        assert_eq!(chunks.len(), 4);
        let m = EvidenceManifestV1::build(h(9), &bond(), &[5u8; 32], h(1), h(2), h(3), 4, 10_000, &chunks);
        fs::publish(&honest, "claimA", &m, &chunks).unwrap();
        // A forger offers a manifest for ANOTHER execution's roots under the same claim id, with chunks that hash to it.
        let forged = EvidenceManifestV1::build(h(9), &bond(), &[5u8; 32], h(1), h(77), h(3), 4, 10_000, &chunks);
        fs::publish(&forger, "claimA", &forged, &chunks).unwrap();
        let mut c = claim();
        c.trace_chunk_count = 4;
        let limits = ManifestLimits::default();
        // The forger is listed FIRST; its manifest disagrees with the claim's roots and is refused, the honest one serves.
        let (got, report) = fs::fetch_claim_material(&[forger.clone(), honest.clone(), empty.clone()], "claimA", &c, &limits, h(4)).unwrap();
        assert_eq!(got, material, "the material is reassembled from verified chunks");
        assert!(report.served_by.iter().all(|p| p.ends_with("honest") || p.ends_with("forger")));
        // Only the forger → nothing admissible; only the empty directory → nothing offered.
        match fs::fetch_claim_material(&[forger], "claimA", &c, &limits, h(4)) {
            Err(fs::ClaimFetchError::NoAdmissibleManifest { offered: 1, refusals }) => assert!(refusals[0].contains("output")),
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            fs::fetch_claim_material(&[empty], "claimA", &c, &limits, h(4)),
            Err(fs::ClaimFetchError::NoAdmissibleManifest { offered: 0, .. })
        ));
        // A corrupted chunk on disk is never accepted, even from a directory whose manifest is admissible.
        let id = manifest_id_v1(&m);
        let victim = fs::chunk_path(&honest, id, 2);
        let mut bytes = std::fs::read(&victim).unwrap();
        bytes[0] ^= 0xFF;
        std::fs::write(&victim, bytes).unwrap();
        assert!(matches!(
            fs::fetch_claim_material(&[honest], "claimA", &c, &limits, h(4)),
            Err(fs::ClaimFetchError::Fetch(FetchFailure::ChunkUnavailable { index: 2, .. }))
        ));
        let _ = std::fs::remove_dir_all(&tmp);
    }

}
