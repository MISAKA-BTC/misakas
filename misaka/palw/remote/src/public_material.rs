//! **Lane DA16 (RFC-0014 §16, RFC-0009 §4): the public material transport — an artifact's bytes and a claim's material, fetchable by any
//! outsider for the liability horizon, every byte checked against the CHAIN's root.**
//!
//! **The artifact half is NON-CONSENSUS, optional off-chain tooling (ADR-0177, 2026-10-10).** Model distribution is off-chain and
//! voluntary: nothing here is a duty, no lease or court backs it, and a provider that serves no model bytes is never charged. A
//! verifier uses it to obtain a model and check it against the REGISTERED roots; the chain never reads its result. The claim half
//! (claim positions, held units) is what the provider court's `ClaimPosition` unit and the kernel's demand serve.
//!
//! [`crate::evidence`] / [`crate::transport`] carry a free-prompt claim's evidence chunks. This module carries the rest of what a fresh
//! verifier needs, over the SAME providers (a directory, HTTP(S), the reference [`crate::transport::server`]) and with the same
//! discipline — nothing a provider says is believed:
//!
//! * **an artifact** (a V2 IR class's inventory leaves, and the kernel side of its binding): an [`ArtifactManifestV1`] carrying the class
//!   record and every leaf hash, accepted only after the record derives the class id asked for over the chain's root and the hashes root
//!   to it; every leaf then checked alone at the coordinates the program fixes. [`fetch_artifact_v1`] is the cold verifier's full fetch;
//!   [`check_binding_v1`] turns the bytes into CONFIRMED or a refutation; [`fetch_artifact_unit_v1`] fetches one kernel-side unit (the
//!   commitments, a row-tree run, a row) verified by the court's own function;
//! * **a kernel claim's positions** ([`ClaimMaterialManifestV1`], verified by the kernel's `Respond` classification against the claim row);
//! * **a V2 held claim's units** (RFC-0014's held class: prompt tiles, checkpoint state chunks, step ranges and leaves —
//!   [`HeldMaterialManifestV1`], verified by `palw_held_da_check_disclosure_v1` against the claim's `execution_root`).
//!
//! Upload is read-back verified, availability is per provider per unit and labelled [`crate::transport::LOCAL_OBSERVATION`], repair
//! re-seeds a thin provider from verified copies, and [`PublicMaterialMonitorV1`] keeps an artifact alive until its retention ends.
//! **None of this is availability evidence for anyone else**: what holds a provider to its promise is the provider court
//! (`kaspa_consensus_core::palw_provider_court_v1`, tags 150–153), whose challenges name exactly these units.
//!
//! Layout (relative to a provider's root; the reference server accepts exactly these shapes):
//!
//! ```text
//! artifacts/<class>/<artifact root>.manifest             ArtifactManifestV1           (written LAST)
//! artifacts/<artifact root>/leaf/<index>.unit            PalwArtifactOperandV1
//! artifacts/<artifact root>/kernel/<kernel root>/<stem>.unit   PublicUnitAnswerV1     (kernel-side units)
//! material/<claim>/manifest                              MaterialManifestV1           (written LAST)
//! material/<claim>/<stem>.unit                           a position response / a held disclosure
//! ```

use std::collections::BTreeMap;
use std::path::Path;

use kaspa_consensus_core::palw_artifact::{PalwArtifactOperandV1, artifact_leaf_v1};
use kaspa_consensus_core::palw_held_da_v1::{PalwHeldDisclosureV1, PalwHeldMissingV1, palw_held_da_check_disclosure_v1};
use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use kaspa_consensus_core::palw_public_material_v1::{
    ArtifactFactsV1, ArtifactManifestV1, BindingCheckV1, PublicUnitAnswerV1, PublicUnitV1, binding_check_from_leaves_v1,
    canonical_leaf_rows_v1, verify_artifact_answer_v1,
};
use kaspa_consensus_core::palw_step_leg::PalwStepBindingV2;
use kaspa_hashes::Hash64;
use misaka_palw_kernel::ledger::KernelLedgerV1;
use misaka_palw_tir::TirProgramV1;

use crate::evidence::ProviderError;
use crate::{finish, keyed, put_len};

pub const PUBLIC_MATERIAL_VERSION_V1: u16 = 1;
const DOMAIN_UNIT_HASH: &[u8] = b"misaka-palw/remote/public-material/unit/v1";
const DOMAIN_ORDER: &[u8] = b"misaka-palw/remote/public-material/fetch-order/v1";
/// The largest manifest a client reads (a 400,000-leaf artifact's hashes are 25.6 MB, plus the class record).
pub const MATERIAL_MANIFEST_MAX_BYTES: usize = 64 << 20;
/// The largest unit a client reads (a leaf is ≤ 32 KiB; a kernel run of 1,024 nodes with the commitments, or a held state chunk, is far
/// less than this).
pub const MATERIAL_UNIT_MAX_BYTES: usize = 16 << 20;

/// The hash a material manifest lists for a unit (an index only: the unit is verified against the chain, never against this alone).
pub fn unit_hash_v1(bytes: &[u8]) -> Hash64 {
    let mut s = keyed(DOMAIN_UNIT_HASH);
    put_len(&mut s, bytes);
    finish(s)
}

// ---------------------------------------------------------------------------------------------------------------------------------
// Paths
// ---------------------------------------------------------------------------------------------------------------------------------

pub fn artifact_manifest_path(v2_class: &Hash64, artifact_root: &Hash64) -> String {
    format!("artifacts/{v2_class}/{artifact_root}.manifest")
}
pub fn artifact_leaf_path(artifact_root: &Hash64, index: u32) -> String {
    format!("artifacts/{artifact_root}/leaf/{index}.unit")
}
pub fn artifact_kernel_unit_path(artifact_root: &Hash64, kernel_param_root: &Hash64, unit: &PublicUnitV1) -> String {
    format!("artifacts/{artifact_root}/kernel/{kernel_param_root}/{}.unit", unit.file_stem())
}
pub fn material_manifest_path(claim: &Hash64) -> String {
    format!("material/{claim}/manifest")
}
pub fn material_unit_path(claim: &Hash64, stem: &str) -> String {
    format!("material/{claim}/{stem}.unit")
}
/// The file stem of a held unit.
pub fn held_unit_stem(missing: &PalwHeldMissingV1) -> String {
    match *missing {
        PalwHeldMissingV1::PromptIdsTile { tile } => format!("held-tile-{tile}"),
        PalwHeldMissingV1::StateChunk { checkpoint, chunk } => format!("held-state-{checkpoint}-{chunk}"),
        PalwHeldMissingV1::StepRange { first, count } => format!("held-range-{first}-{count}"),
        PalwHeldMissingV1::StepLeaf { leaf } => format!("held-leaf-{leaf}"),
    }
}

fn is_hex128(s: &str) -> bool {
    s.len() == 128 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

fn is_stem(s: &str) -> bool {
    !s.is_empty() && s.len() <= 96 && s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// **Is `path` one of the layout's shapes?** (the reference server's strict parser; nothing else reaches its filesystem).
pub fn is_material_path(path: &str) -> bool {
    let parts: Vec<&str> = path.split('/').collect();
    match parts.as_slice() {
        ["artifacts", class, file] => is_hex128(class) && file.strip_suffix(".manifest").is_some_and(is_hex128),
        ["artifacts", root, "leaf", file] => {
            is_hex128(root)
                && file.strip_suffix(".unit").is_some_and(|i| !i.is_empty() && i.len() <= 10 && i.bytes().all(|b| b.is_ascii_digit()))
        }
        ["artifacts", root, "kernel", kroot, file] => {
            is_hex128(root) && is_hex128(kroot) && file.strip_suffix(".unit").is_some_and(is_stem)
        }
        ["material", claim, "manifest"] => is_hex128(claim),
        ["material", claim, file] => is_hex128(claim) && file.strip_suffix(".unit").is_some_and(is_stem),
        _ => false,
    }
}

// ---------------------------------------------------------------------------------------------------------------------------------
// The provider seam
// ---------------------------------------------------------------------------------------------------------------------------------

/// A provider of public material: get / put by layout path. `Err` is a local failure, never evidence; `Ok(None)` is "it holds none".
pub trait MaterialProvider {
    fn material_id(&self) -> String;
    fn get(&self, path: &str, max: usize) -> Result<Option<Vec<u8>>, ProviderError>;
    fn put(&self, path: &str, bytes: &[u8]) -> Result<(), ProviderError>;
}

impl MaterialProvider for crate::evidence::fs::FsProvider {
    fn material_id(&self) -> String {
        self.id.clone()
    }
    fn get(&self, path: &str, max: usize) -> Result<Option<Vec<u8>>, ProviderError> {
        if !is_material_path(path) {
            return Err(ProviderError(format!("{path:?} is not a material path")));
        }
        match std::fs::read(self.root.join(path)) {
            Ok(bytes) if bytes.len() > max => Err(ProviderError(format!("{path}: {} bytes, more than asked for", bytes.len()))),
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(ProviderError(e.to_string())),
        }
    }
    fn put(&self, path: &str, bytes: &[u8]) -> Result<(), ProviderError> {
        if !is_material_path(path) {
            return Err(ProviderError(format!("{path:?} is not a material path")));
        }
        let full = self.root.join(path);
        if let Some(dir) = full.parent() {
            std::fs::create_dir_all(dir).map_err(|e| ProviderError(e.to_string()))?;
        }
        let tmp = full.with_extension("partial");
        std::fs::write(&tmp, bytes).and_then(|()| std::fs::rename(&tmp, &full)).map_err(|e| ProviderError(e.to_string()))
    }
}

impl MaterialProvider for crate::transport::HttpProvider {
    fn material_id(&self) -> String {
        self.base.clone()
    }
    fn get(&self, path: &str, max: usize) -> Result<Option<Vec<u8>>, ProviderError> {
        let r = self.http.request("GET", &format!("{}/{path}", self.base), None, max).map_err(ProviderError)?;
        match r.status {
            200 => Ok(Some(r.body)),
            404 => Ok(None),
            other => Err(ProviderError(format!("HTTP {other}"))),
        }
    }
    fn put(&self, path: &str, bytes: &[u8]) -> Result<(), ProviderError> {
        let r = self.http.request("PUT", &format!("{}/{path}", self.base), Some(bytes), 4096).map_err(ProviderError)?;
        if !(200..300).contains(&r.status) {
            return Err(ProviderError(format!("HTTP {}: {}", r.status, String::from_utf8_lossy(&r.body))));
        }
        Ok(())
    }
}

/// The providers a list names, as material providers.
pub fn open_material_providers_v1(specs: &[crate::transport::ProviderSpecV1]) -> Vec<Box<dyn MaterialProvider>> {
    specs
        .iter()
        .map(|spec| -> Box<dyn MaterialProvider> {
            match spec {
                crate::transport::ProviderSpecV1::Dir(p) => Box::new(crate::evidence::fs::FsProvider::new(p.clone())),
                crate::transport::ProviderSpecV1::Http(u) => Box::new(crate::transport::HttpProvider::new(u.clone())),
            }
        })
        .collect()
}

/// The order providers are tried for a unit: deterministic in (seed, unit, provider), so a replay tries the same order and many verifiers do
/// not all hammer the first provider.
fn order_for(seed: Hash64, unit: &str, providers: &[&dyn MaterialProvider]) -> Vec<usize> {
    let mut keyed_order: Vec<(Hash64, usize)> = providers
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let mut s = keyed(DOMAIN_ORDER);
            s.update(seed.as_bytes().as_slice());
            put_len(&mut s, unit.as_bytes());
            put_len(&mut s, p.material_id().as_bytes());
            (finish(s), i)
        })
        .collect();
    keyed_order.sort();
    keyed_order.into_iter().map(|(_, i)| i).collect()
}

/// One unit from ANY provider: the first whose bytes `accept` takes. Every refusal is recorded `(provider, why)`, never believed.
pub fn fetch_unit_any<T>(
    providers: &[&dyn MaterialProvider],
    path: &str,
    max: usize,
    seed: Hash64,
    mut accept: impl FnMut(&[u8]) -> Result<T, String>,
) -> Result<(T, String), Vec<(String, String)>> {
    let mut failures = Vec::new();
    for i in order_for(seed, path, providers) {
        let p = providers[i];
        match p.get(path, max) {
            Ok(Some(bytes)) => match accept(&bytes) {
                Ok(v) => return Ok((v, p.material_id())),
                Err(why) => failures.push((p.material_id(), format!("bad bytes: {why}"))),
            },
            Ok(None) => failures.push((p.material_id(), "absent".to_string())),
            Err(e) => failures.push((p.material_id(), format!("unreachable: {e}"))),
        }
    }
    Err(failures)
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MaterialFetchErrorV1 {
    #[error("no provider holds a manifest the chain's roots accept (refused: {0:?})")]
    NoAdmissibleManifest(Vec<(String, String)>),
    #[error("no provider served unit {unit} that verifies (tried: {failures:?})")]
    UnitUnavailable { unit: String, failures: Vec<(String, String)> },
    #[error("{0}")]
    Invalid(String),
}

/// Who served what, and every refusal on the way — the raw material of a provider-court challenge (and of nothing else).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MaterialFetchReportV1 {
    pub served_by: Vec<String>,
    pub failures: Vec<(String, String, String)>,
}

// ---------------------------------------------------------------------------------------------------------------------------------
// Artifacts
// ---------------------------------------------------------------------------------------------------------------------------------

/// What [`fetch_artifact_v1`] returns: the checked manifest, the program, every leaf in inventory order.
#[derive(Clone, Debug)]
pub struct FetchedArtifactV1 {
    pub manifest: ArtifactManifestV1,
    pub program: TirProgramV1,
    pub leaves: Vec<PalwArtifactOperandV1>,
    pub report: MaterialFetchReportV1,
}

impl FetchedArtifactV1 {
    /// The facts the court judges this artifact's units against.
    pub fn facts(&self) -> ArtifactFactsV1<'_> {
        ArtifactFactsV1 {
            program: &self.program,
            artifact_root: self.manifest.artifact_root,
            kernel_param_root: self.manifest.kernel_param_root,
        }
    }
}

fn decode_exact<T: borsh::BorshDeserialize>(bytes: &[u8]) -> Result<T, String> {
    let mut slice = bytes;
    let v = T::deserialize(&mut slice).map_err(|e| format!("does not decode: {e}"))?;
    slice.is_empty().then_some(v).ok_or_else(|| "trailing bytes".to_string())
}

/// **The artifact's manifest from any provider** — the first the chain's facts accept (`kernel_param_root: None` asks for the V2 bytes alone).
pub fn fetch_artifact_manifest_v1(
    providers: &[&dyn MaterialProvider],
    network_domain: Hash64,
    v2_class: Hash64,
    artifact_root: Hash64,
    kernel_param_root: Option<Hash64>,
) -> Result<(ArtifactManifestV1, TirProgramV1), MaterialFetchErrorV1> {
    let path = artifact_manifest_path(&v2_class, &artifact_root);
    let mut refused = Vec::new();
    for p in providers {
        match p.get(&path, MATERIAL_MANIFEST_MAX_BYTES) {
            Ok(Some(bytes)) => match decode_exact::<ArtifactManifestV1>(&bytes)
                .and_then(|m| m.check_against_chain(network_domain, v2_class, artifact_root, kernel_param_root).map(|prog| (m, prog)))
            {
                Ok(pair) => return Ok(pair),
                Err(why) => refused.push((p.material_id(), why)),
            },
            Ok(None) => refused.push((p.material_id(), "absent".to_string())),
            Err(e) => refused.push((p.material_id(), format!("unreachable: {e}"))),
        }
    }
    Err(MaterialFetchErrorV1::NoAdmissibleManifest(refused))
}

/// **The whole artifact, cold, from any providers** (RFC-0014 §16.6's cold verifier): the manifest checked against the chain, then every
/// leaf from any provider, each checked alone (hash and canonical coordinates).
pub fn fetch_artifact_v1(
    providers: &[&dyn MaterialProvider],
    network_domain: Hash64,
    v2_class: Hash64,
    artifact_root: Hash64,
    kernel_param_root: Option<Hash64>,
    seed: Hash64,
) -> Result<FetchedArtifactV1, MaterialFetchErrorV1> {
    let (manifest, program) = fetch_artifact_manifest_v1(providers, network_domain, v2_class, artifact_root, kernel_param_root)?;
    let rows = canonical_leaf_rows_v1(&program).map_err(MaterialFetchErrorV1::Invalid)?;
    let mut leaves = Vec::with_capacity(rows.len());
    let mut report = MaterialFetchReportV1::default();
    for (i, row) in rows.iter().enumerate() {
        let path = artifact_leaf_path(&artifact_root, i as u32);
        let got = fetch_unit_any(providers, &path, MATERIAL_UNIT_MAX_BYTES, seed, |bytes| {
            let operand = decode_exact::<PalwArtifactOperandV1>(bytes)?;
            manifest.verify_leaf(&program, row, i as u32, &operand)?;
            Ok(operand)
        });
        match got {
            Ok((operand, by)) => {
                leaves.push(operand);
                report.served_by.push(by);
            }
            Err(failures) => return Err(MaterialFetchErrorV1::UnitUnavailable { unit: path, failures }),
        }
    }
    Ok(FetchedArtifactV1 { manifest, program, leaves, report })
}

/// **Confirm or refute the binding the manifest names, from the fetched bytes** (`palw_public_material_v1::binding_check_from_leaves_v1`).
pub fn check_binding_v1(fetched: &FetchedArtifactV1, kernel_param_root: Hash64) -> Result<BindingCheckV1, String> {
    binding_check_from_leaves_v1(&fetched.program, &fetched.leaves, fetched.manifest.artifact_root, kernel_param_root)
}

/// **One kernel-side unit of an artifact pair from any provider**, verified by the court's own function against the chain's roots.
pub fn fetch_artifact_unit_v1(
    providers: &[&dyn MaterialProvider],
    facts: &ArtifactFactsV1<'_>,
    unit: &PublicUnitV1,
    seed: Hash64,
) -> Result<PublicUnitAnswerV1, MaterialFetchErrorV1> {
    let path = match unit {
        PublicUnitV1::ArtifactLeaf { .. } | PublicUnitV1::ClaimPosition { .. } => {
            return Err(MaterialFetchErrorV1::Invalid("a leaf is fetched with the artifact; a position with the claim".to_string()));
        }
        _ => artifact_kernel_unit_path(&facts.artifact_root, &facts.kernel_param_root, unit),
    };
    fetch_unit_any(providers, &path, MATERIAL_UNIT_MAX_BYTES, seed, |bytes| {
        let answer = decode_exact::<PublicUnitAnswerV1>(bytes)?;
        verify_artifact_answer_v1(facts, unit, &answer).map_err(str::to_string)?;
        Ok(answer)
    })
    .map(|(a, _)| a)
    .map_err(|failures| MaterialFetchErrorV1::UnitUnavailable { unit: path, failures })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterialPublishOutcomeV1 {
    pub provider: String,
    /// `Ok`: every unit and the manifest accepted AND read back verified.
    pub result: Result<(), String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterialPublishReportV1 {
    pub per_provider: Vec<MaterialPublishOutcomeV1>,
}

impl MaterialPublishReportV1 {
    pub fn verified_copies(&self) -> usize {
        self.per_provider.iter().filter(|o| o.result.is_ok()).count()
    }
}

fn publish_with_read_back(
    providers: &[&dyn MaterialProvider],
    units: &[(String, Vec<u8>)],
    manifest: (String, Vec<u8>),
    min_verified: usize,
) -> Result<MaterialPublishReportV1, MaterialPublishReportV1> {
    let mut per_provider = Vec::new();
    for p in providers {
        let result = (|| -> Result<(), String> {
            for (path, bytes) in units {
                p.put(path, bytes).map_err(|e| format!("{path}: {e}"))?;
            }
            p.put(&manifest.0, &manifest.1).map_err(|e| format!("manifest: {e}"))?;
            // Read back what the provider now serves: an ACK is its word, a read-back is what this machine saw.
            for (path, bytes) in units.iter().chain(std::iter::once(&manifest)) {
                match p.get(path, bytes.len().max(1)).map_err(|e| format!("read-back {path}: {e}"))? {
                    Some(got) if got == *bytes => {}
                    Some(_) => return Err(format!("read-back {path}: other bytes")),
                    None => return Err(format!("read-back {path}: absent after accepting it")),
                }
            }
            Ok(())
        })();
        per_provider.push(MaterialPublishOutcomeV1 { provider: p.material_id(), result });
    }
    let report = MaterialPublishReportV1 { per_provider };
    if report.verified_copies() >= min_verified.max(1) { Ok(report) } else { Err(report) }
}

/// **Place an artifact with several providers**: every leaf and every kernel-side unit first, the manifest LAST, all read back. `Err`
/// when fewer than `min_verified` providers hold a verified copy. The manifest is checked against its own leaves first (a publisher cannot
/// place a manifest its bytes do not back).
pub fn publish_artifact_v1(
    providers: &[&dyn MaterialProvider],
    manifest: &ArtifactManifestV1,
    leaves: &[PalwArtifactOperandV1],
    kernel_units: &[(PublicUnitV1, PublicUnitAnswerV1)],
    min_verified: usize,
) -> Result<MaterialPublishReportV1, String> {
    let program = manifest.check_against_chain(manifest.network_domain, manifest.v2_class(), manifest.artifact_root, None)?;
    let rows = canonical_leaf_rows_v1(&program)?;
    if rows.len() != leaves.len() {
        return Err(format!("{} leaves, the program's inventory has {}", leaves.len(), rows.len()));
    }
    let facts =
        ArtifactFactsV1 { program: &program, artifact_root: manifest.artifact_root, kernel_param_root: manifest.kernel_param_root };
    let mut units = Vec::with_capacity(leaves.len() + kernel_units.len());
    for (i, (row, leaf)) in rows.iter().zip(leaves).enumerate() {
        manifest.verify_leaf(&program, row, i as u32, leaf)?;
        units.push((artifact_leaf_path(&manifest.artifact_root, i as u32), borsh::to_vec(leaf).expect("an operand serializes")));
    }
    for (unit, answer) in kernel_units {
        verify_artifact_answer_v1(&facts, unit, answer).map_err(|e| format!("{unit:?}: {e}"))?;
        units.push((
            artifact_kernel_unit_path(&manifest.artifact_root, &manifest.kernel_param_root, unit),
            borsh::to_vec(answer).expect("an answer serializes"),
        ));
    }
    let m = (
        artifact_manifest_path(&manifest.v2_class(), &manifest.artifact_root),
        borsh::to_vec(manifest).expect("a manifest serializes"),
    );
    publish_with_read_back(providers, &units, m, min_verified).map_err(|r| format!("too few verified copies: {:?}", r.per_provider))
}

/// One provider's standing for an artifact, as THIS machine saw it ([`crate::transport::LOCAL_OBSERVATION`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtifactAvailabilityV1 {
    pub provider: String,
    pub manifest_ok: bool,
    pub verified: usize,
    pub missing: Vec<u32>,
    pub corrupt: Vec<u32>,
    pub unreachable: Option<String>,
}

/// **Per provider, per leaf**: verified / missing / corrupt / unreachable. Never a slash reason, never proof for anybody else.
pub fn artifact_availability_v1(
    providers: &[&dyn MaterialProvider],
    manifest: &ArtifactManifestV1,
    program: &TirProgramV1,
) -> Vec<ArtifactAvailabilityV1> {
    let rows = canonical_leaf_rows_v1(program).unwrap_or_default();
    let mpath = artifact_manifest_path(&manifest.v2_class(), &manifest.artifact_root);
    providers
        .iter()
        .map(|p| {
            let mut a = ArtifactAvailabilityV1 {
                provider: p.material_id(),
                manifest_ok: false,
                verified: 0,
                missing: Vec::new(),
                corrupt: Vec::new(),
                unreachable: None,
            };
            match p.get(&mpath, MATERIAL_MANIFEST_MAX_BYTES) {
                Ok(Some(bytes)) => a.manifest_ok = decode_exact::<ArtifactManifestV1>(&bytes).is_ok_and(|m| m == *manifest),
                Ok(None) => {}
                Err(e) => {
                    a.unreachable = Some(e.0);
                    return a;
                }
            }
            for (i, row) in rows.iter().enumerate() {
                match p.get(&artifact_leaf_path(&manifest.artifact_root, i as u32), MATERIAL_UNIT_MAX_BYTES) {
                    Ok(Some(bytes)) => {
                        let ok = decode_exact::<PalwArtifactOperandV1>(&bytes)
                            .and_then(|op| manifest.verify_leaf(program, row, i as u32, &op))
                            .is_ok();
                        if ok { a.verified += 1 } else { a.corrupt.push(i as u32) }
                    }
                    Ok(None) => a.missing.push(i as u32),
                    Err(e) => {
                        a.unreachable = Some(e.0);
                        break;
                    }
                }
            }
            a
        })
        .collect()
}

/// **Re-seed every thin provider from verified copies** (anyone holding the bytes may — the artifact outlives its publisher's uptime).
/// Returns how many providers now hold a complete verified copy (read back).
pub fn repair_artifact_v1(providers: &[&dyn MaterialProvider], manifest: &ArtifactManifestV1, seed: Hash64) -> Result<usize, String> {
    let fetched = fetch_artifact_v1(providers, manifest.network_domain, manifest.v2_class(), manifest.artifact_root, None, seed)
        .map_err(|e| e.to_string())?;
    let thin: Vec<&dyn MaterialProvider> = artifact_availability_v1(providers, manifest, &fetched.program)
        .iter()
        .zip(providers)
        .filter(|(a, _)| a.unreachable.is_none() && (!a.manifest_ok || a.verified < fetched.leaves.len()))
        .map(|(_, p)| *p)
        .collect();
    if !thin.is_empty() {
        let _ = publish_artifact_v1(&thin, manifest, &fetched.leaves, &[], 1);
    }
    Ok(artifact_availability_v1(providers, manifest, &fetched.program)
        .iter()
        .filter(|a| a.manifest_ok && a.verified == fetched.leaves.len())
        .count())
}

/// **The root-fetch hook's output** (`kaspad --palw-root-fetch-cmd`): the fetched, verified artifact written as a `PALWTIR1` container
/// (program, layout, tokenizer id and every tensor, from the manifest's class record and the leaves), which the node's bundle loader reads.
pub fn write_container_v1(path: &Path, fetched: &FetchedArtifactV1) -> Result<[u8; 64], String> {
    let rows = canonical_leaf_rows_v1(&fetched.program)?;
    let mut tensors: BTreeMap<(u16, Option<u16>), Vec<u8>> = BTreeMap::new();
    for (row, leaf) in rows.iter().zip(&fetched.leaves) {
        tensors.entry((row.param, row.layer)).or_default().extend_from_slice(&leaf.bytes);
    }
    let class = &fetched.manifest.class;
    misaka_palw_tir_artifact::write_container_v1(
        path,
        &fetched.program,
        borsh::to_vec(&class.layout).expect("a layout serializes"),
        class.tokenizer_id.as_bytes(),
        // JSON, as every container's meta is (the bundle loader reads it as an object).
        serde_json::json!({
            "provenance": "fetched by palw-evidence from public providers, every leaf checked against the chain's artifact root",
            "artifact_root": fetched.manifest.artifact_root.to_string(),
            "kernel_param_root": fetched.manifest.kernel_param_root.to_string(),
        })
        .to_string(),
        &mut |param, layer| tensors.remove(&(param, layer)).ok_or_else(|| format!("no bytes for param {param} layer {layer:?}")),
    )
    .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------------------------------------------------------------
// Claim material: kernel claim positions and V2 held units
// ---------------------------------------------------------------------------------------------------------------------------------

/// One unit a claim-material manifest lists.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct MaterialEntryV1 {
    pub stem: String,
    pub len: u32,
    pub hash: Hash64,
}

/// **The index of a claim's public material** — a kernel route claim's positions, or a V2 held claim's units (with the claim's binding,
/// authenticated against its `execution_root` before any unit is believed).
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum MaterialManifestV1 {
    KernelClaim {
        version: u16,
        network_domain: Hash64,
        claim: Hash64,
        positions: Vec<(u8, u32)>,
        entries: Vec<MaterialEntryV1>,
        retain_until_daa: u64,
    },
    HeldClaim {
        version: u16,
        network_domain: Hash64,
        claim: Hash64,
        execution_root: Hash64,
        binding: Box<PalwStepBindingV2>,
        units: Vec<PalwHeldMissingV1>,
        entries: Vec<MaterialEntryV1>,
        retain_until_daa: u64,
    },
}

impl MaterialManifestV1 {
    pub fn claim(&self) -> Hash64 {
        match self {
            Self::KernelClaim { claim, .. } | Self::HeldClaim { claim, .. } => *claim,
        }
    }
}

/// **Place a kernel claim's positions** (`(stage, position, PositionResponseV1 bytes)`), each checked first by the kernel's own
/// classification against the claim row of `ledger` (a provider never places material that would not serve a demand).
pub fn publish_claim_positions_v1(
    providers: &[&dyn MaterialProvider],
    network_domain: Hash64,
    ledger: &KernelLedgerV1,
    claim: Hash64,
    positions: &[(u8, u32, Vec<u8>)],
    retain_until_daa: u64,
    min_verified: usize,
) -> Result<MaterialPublishReportV1, String> {
    let mut units = Vec::new();
    let mut entries = Vec::new();
    for (stage, position, bytes) in positions {
        ledger
            .classify_served_position_v1(&claim.as_bytes(), *stage, *position, bytes)
            .map_err(|e| format!("position {stage}/{position}: {e}"))?;
        let stem = PublicUnitV1::ClaimPosition { stage: *stage, position: *position }.file_stem();
        units.push((material_unit_path(&claim, &stem), bytes.clone()));
        entries.push(MaterialEntryV1 { stem, len: bytes.len() as u32, hash: unit_hash_v1(bytes) });
    }
    let manifest = MaterialManifestV1::KernelClaim {
        version: PUBLIC_MATERIAL_VERSION_V1,
        network_domain,
        claim,
        positions: positions.iter().map(|(s, p, _)| (*s, *p)).collect(),
        entries,
        retain_until_daa,
    };
    let m = (material_manifest_path(&claim), borsh::to_vec(&manifest).expect("a manifest serializes"));
    publish_with_read_back(providers, &units, m, min_verified).map_err(|r| format!("too few verified copies: {:?}", r.per_provider))
}

/// **One kernel claim position from any provider**, accepted only if the kernel's own `Respond` classification serves it against the
/// claim row of `ledger` (the fresh verifier's ledger, rebuilt from the node's public rows).
pub fn fetch_claim_position_v1(
    providers: &[&dyn MaterialProvider],
    ledger: &KernelLedgerV1,
    claim: Hash64,
    stage: u8,
    position: u32,
    seed: Hash64,
) -> Result<Vec<u8>, MaterialFetchErrorV1> {
    let path = material_unit_path(&claim, &PublicUnitV1::ClaimPosition { stage, position }.file_stem());
    fetch_unit_any(providers, &path, MATERIAL_UNIT_MAX_BYTES, seed, |bytes| {
        ledger.classify_served_position_v1(&claim.as_bytes(), stage, position, bytes).map_err(str::to_string)?;
        Ok(bytes.to_vec())
    })
    .map(|(b, _)| b)
    .map_err(|failures| MaterialFetchErrorV1::UnitUnavailable { unit: path, failures })
}

/// The chain facts a held unit is judged against: the claim's `execution_root`, the network's ladder and prompt-ids form.
#[derive(Clone, Copy, Debug)]
pub struct HeldFactsV1 {
    pub execution_root: Hash64,
    pub ladder: u64,
    pub prompt_form: PalwPromptIdsFormV1,
}

/// **Place a V2 held claim's units** (RFC-0014's held class), each checked first by `palw_held_da_check_disclosure_v1` against the
/// claim's committed roots.
#[allow(clippy::too_many_arguments)]
pub fn publish_held_material_v1(
    providers: &[&dyn MaterialProvider],
    network_domain: Hash64,
    claim: Hash64,
    facts: HeldFactsV1,
    binding: &PalwStepBindingV2,
    units: &[(PalwHeldMissingV1, PalwHeldDisclosureV1)],
    retain_until_daa: u64,
    min_verified: usize,
) -> Result<MaterialPublishReportV1, String> {
    let mut paths = Vec::new();
    let mut entries = Vec::new();
    for (missing, disclosure) in units {
        palw_held_da_check_disclosure_v1(&facts.execution_root, missing, binding, disclosure, facts.ladder, facts.prompt_form)
            .map_err(|e| format!("{missing:?}: {e}"))?;
        let bytes = borsh::to_vec(disclosure).expect("a disclosure serializes");
        let stem = held_unit_stem(missing);
        entries.push(MaterialEntryV1 { stem: stem.clone(), len: bytes.len() as u32, hash: unit_hash_v1(&bytes) });
        paths.push((material_unit_path(&claim, &stem), bytes));
    }
    let manifest = MaterialManifestV1::HeldClaim {
        version: PUBLIC_MATERIAL_VERSION_V1,
        network_domain,
        claim,
        execution_root: facts.execution_root,
        binding: Box::new(binding.clone()),
        units: units.iter().map(|(m, _)| *m).collect(),
        entries,
        retain_until_daa,
    };
    let m = (material_manifest_path(&claim), borsh::to_vec(&manifest).expect("a manifest serializes"));
    publish_with_read_back(providers, &paths, m, min_verified).map_err(|r| format!("too few verified copies: {:?}", r.per_provider))
}

/// **One held unit from any provider**: the binding from any provider's manifest that names this claim's `execution_root` (the binding is
/// authenticated against it by the check itself), then the unit, accepted only if `palw_held_da_check_disclosure_v1` takes it.
pub fn fetch_held_unit_v1(
    providers: &[&dyn MaterialProvider],
    claim: Hash64,
    facts: HeldFactsV1,
    missing: PalwHeldMissingV1,
    seed: Hash64,
) -> Result<PalwHeldDisclosureV1, MaterialFetchErrorV1> {
    let mut refused = Vec::new();
    let mut binding = None;
    for p in providers {
        match p.get(&material_manifest_path(&claim), MATERIAL_MANIFEST_MAX_BYTES) {
            Ok(Some(bytes)) => match decode_exact::<MaterialManifestV1>(&bytes) {
                Ok(MaterialManifestV1::HeldClaim { claim: c, execution_root, binding: b, .. })
                    if c == claim && execution_root == facts.execution_root =>
                {
                    binding = Some(*b);
                    break;
                }
                Ok(_) => refused.push((p.material_id(), "another claim's or kind's manifest".to_string())),
                Err(e) => refused.push((p.material_id(), e)),
            },
            Ok(None) => refused.push((p.material_id(), "absent".to_string())),
            Err(e) => refused.push((p.material_id(), format!("unreachable: {e}"))),
        }
    }
    let binding = binding.ok_or(MaterialFetchErrorV1::NoAdmissibleManifest(refused))?;
    let path = material_unit_path(&claim, &held_unit_stem(&missing));
    fetch_unit_any(providers, &path, MATERIAL_UNIT_MAX_BYTES, seed, |bytes| {
        let disclosure = decode_exact::<PalwHeldDisclosureV1>(bytes)?;
        palw_held_da_check_disclosure_v1(&facts.execution_root, &missing, &binding, &disclosure, facts.ladder, facts.prompt_form)
            .map_err(|e| e.to_string())?;
        Ok(disclosure)
    })
    .map(|(d, _)| d)
    .map_err(|failures| MaterialFetchErrorV1::UnitUnavailable { unit: path, failures })
}

// ---------------------------------------------------------------------------------------------------------------------------------
// Retention
// ---------------------------------------------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MaterialRetentionEventV1 {
    /// At least `required` providers hold a complete verified copy (`LOCAL_OBSERVATION`).
    Healthy { artifact_root: Hash64, copies: usize },
    /// Fewer than `required`; repaired up to `copies`.
    Repaired { artifact_root: Hash64, copies: usize },
    /// Nothing complete to copy from: the artifact is at risk (loud).
    AtRisk { artifact_root: Hash64, why: String },
    /// Its retention promise has run out: no longer watched.
    Expired { artifact_root: Hash64 },
}

/// **Keeps watched artifacts alive until their retention ends** — anyone may run it (a provider, a watcher, the publisher before it
/// switches off): each tick it counts complete verified copies and repairs below `required`.
#[derive(Default)]
pub struct PublicMaterialMonitorV1 {
    pub required: usize,
    watched: BTreeMap<Hash64, ArtifactManifestV1>,
}

impl PublicMaterialMonitorV1 {
    pub fn new(required: usize) -> Self {
        Self { required: required.max(1), watched: BTreeMap::new() }
    }
    pub fn watch(&mut self, manifest: ArtifactManifestV1) {
        self.watched.insert(manifest.artifact_root, manifest);
    }
    pub fn watching(&self) -> usize {
        self.watched.len()
    }
    pub fn tick(&mut self, now_daa: u64, providers: &[&dyn MaterialProvider], seed: Hash64) -> Vec<MaterialRetentionEventV1> {
        let mut out = Vec::new();
        let expired: Vec<Hash64> = self.watched.values().filter(|m| now_daa > m.retain_until_daa).map(|m| m.artifact_root).collect();
        for root in expired {
            self.watched.remove(&root);
            out.push(MaterialRetentionEventV1::Expired { artifact_root: root });
        }
        for manifest in self.watched.values() {
            let root = manifest.artifact_root;
            let Ok(program) = manifest.program() else { continue };
            let complete = |rows: &[ArtifactAvailabilityV1]| {
                let leaves = manifest.leaf_hashes.len();
                rows.iter().filter(|a| a.manifest_ok && a.verified == leaves).count()
            };
            let copies = complete(&artifact_availability_v1(providers, manifest, &program));
            if copies >= self.required {
                out.push(MaterialRetentionEventV1::Healthy { artifact_root: root, copies });
                continue;
            }
            match repair_artifact_v1(providers, manifest, seed) {
                Ok(n) if n > 0 => out.push(MaterialRetentionEventV1::Repaired { artifact_root: root, copies: n }),
                Ok(_) => {
                    out.push(MaterialRetentionEventV1::AtRisk { artifact_root: root, why: "no complete copy anywhere".to_string() })
                }
                Err(why) => out.push(MaterialRetentionEventV1::AtRisk { artifact_root: root, why }),
            }
        }
        out
    }
}

/// The leaves of an artifact, from a tensor source, in inventory order (what a publisher places).
pub fn artifact_leaves_of_v1(
    program: &TirProgramV1,
    src: &dyn kaspa_consensus_core::palw_tir_artifact_v1::PalwTirTensorSourceV1,
) -> Result<Vec<PalwArtifactOperandV1>, String> {
    kaspa_consensus_core::palw_tir_artifact_v1::palw_tir_inventory_operands_v1(program, src).map_err(|e| e.to_string())
}

/// The leaf hashes of `leaves` (what an [`ArtifactManifestV1`] lists).
pub fn leaf_hashes_v1(leaves: &[PalwArtifactOperandV1]) -> Vec<Hash64> {
    leaves.iter().map(artifact_leaf_v1).collect()
}

/// **What the reference server stores** (its PUT gate for the material layout): a unit as given (every reader judges it against the
/// chain); an artifact manifest only if it is self-consistent (its class record derives the class in its path over the root in its path,
/// its hashes root there) AND every leaf it names is already stored and checks; a claim-material manifest only if every unit it lists is
/// already stored with that hash. The server has no chain: this is "never serve a manifest you cannot back", not a verdict.
pub fn server_accepts_material_v1(root: &Path, path: &str, body: &[u8]) -> Result<(), String> {
    let parts: Vec<&str> = path.split('/').collect();
    match parts.as_slice() {
        ["artifacts", class, file] if file.ends_with(".manifest") => {
            let class: Hash64 = class.parse().map_err(|_| "a class id in the path".to_string())?;
            let artifact_root: Hash64 = file.trim_end_matches(".manifest").parse().map_err(|_| "a root in the path".to_string())?;
            let m = decode_exact::<ArtifactManifestV1>(body)?;
            let program = m.check_against_chain(m.network_domain, class, artifact_root, None)?;
            let rows = canonical_leaf_rows_v1(&program)?;
            for (i, row) in rows.iter().enumerate() {
                let bytes = std::fs::read(root.join(artifact_leaf_path(&artifact_root, i as u32)))
                    .map_err(|_| format!("leaf {i} is not stored here"))?;
                let operand = decode_exact::<PalwArtifactOperandV1>(&bytes)?;
                m.verify_leaf(&program, row, i as u32, &operand)?;
            }
            Ok(())
        }
        ["material", claim, "manifest"] => {
            let claim: Hash64 = claim.parse().map_err(|_| "a claim id in the path".to_string())?;
            let m = decode_exact::<MaterialManifestV1>(body)?;
            if m.claim() != claim {
                return Err("the manifest names another claim than its path".to_string());
            }
            let entries = match &m {
                MaterialManifestV1::KernelClaim { entries, .. } | MaterialManifestV1::HeldClaim { entries, .. } => entries,
            };
            for e in entries {
                let bytes = std::fs::read(root.join(material_unit_path(&claim, &e.stem)))
                    .map_err(|_| format!("{} is not stored here", e.stem))?;
                if bytes.len() != e.len as usize || unit_hash_v1(&bytes) != e.hash {
                    return Err(format!("{} does not match the manifest", e.stem));
                }
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_public_material_v1::{
        answer_artifact_unit_v1, differing_instances_v1, differing_rows_v1, row_refutation_v1,
    };
    use kaspa_consensus_core::palw_tir_artifact_v1::PalwTirTensorSourceV1;
    use misaka_palw_kernel::trace::ParamCommitmentsV1;
    use std::borrow::Cow;

    struct Src(BTreeMap<(u16, Option<u16>), Vec<u8>>);
    impl PalwTirTensorSourceV1 for Src {
        fn tensor_bytes(&self, param: u16, layer: Option<u16>) -> Option<Cow<'_, [u8]>> {
            self.0.get(&(param, layer)).map(|b| Cow::Borrowed(b.as_slice()))
        }
    }

    struct Art {
        manifest: ArtifactManifestV1,
        leaves: Vec<PalwArtifactOperandV1>,
        params: misaka_palw_tir::MapParams,
        pc: ParamCommitmentsV1,
    }

    fn class_of(program: &TirProgramV1) -> kaspa_consensus_core::palw_tir_class_v1::PalwTirClassV1 {
        use kaspa_consensus_core::palw_tir_class_v1::*;
        PalwTirClassV1 {
            version: PALW_TIR_CLASS_VERSION_V1,
            program: program.encode(),
            layout: PalwTirLayoutV1 {
                version: PALW_TIR_LAYOUT_VERSION_V1,
                max_context: 64,
                checkpoint_interval: 2,
                h_tile: 2,
                commit_tiles: Vec::new(),
                state_tiles: Vec::new(),
            },
            tokenizer_id: Hash64::from_bytes([0x70; 64]),
        }
    }

    /// The artifact of `seed`'s weights, its manifest naming `bound`'s kernel root (the pair a provider vouches for).
    fn art(seed: u64, bound: Option<&ParamCommitmentsV1>) -> Art {
        let fx = misaka_palw_tir_sketch::fixture::wide128_v1(seed);
        let src = Src(fx.params.tensors.iter().map(|(k, t)| (*k, t.to_le_bytes())).collect());
        let leaves = artifact_leaves_of_v1(&fx.program, &src).unwrap();
        let root = kaspa_consensus_core::palw_artifact::artifact_root_v1(&leaf_hashes_v1(&leaves)).unwrap();
        let pc = ParamCommitmentsV1::of(&fx.params);
        let kernel_root = Hash64::from_bytes(bound.unwrap_or(&pc).root());
        let manifest = ArtifactManifestV1::of(Hash64::from_bytes([9; 64]), class_of(&fx.program), root, kernel_root, &leaves, 1_000);
        Art { manifest, leaves, params: fx.params, pc }
    }

    fn dirs(tag: &str, n: usize) -> (std::path::PathBuf, Vec<crate::evidence::fs::FsProvider>) {
        let base = std::env::temp_dir().join(format!("da16-material-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let ps = (0..n).map(|i| crate::evidence::fs::FsProvider::new(base.join(format!("p{i}")))).collect();
        (base, ps)
    }

    /// **An outsider fetches an artifact cold from providers it chose — one dead, one lying — and confirms the binding from the bytes;
    /// the same bytes against another artifact's kernel root are refuted through kernel units the pair's provider must serve.**
    #[test]
    fn an_outsider_fetches_the_artifact_and_confirms_or_refutes_the_binding_from_the_bytes() {
        let a = art(11, None);
        let (base, ps) = dirs("confirm", 3);
        let refs: Vec<&dyn MaterialProvider> = ps.iter().map(|p| p as &dyn MaterialProvider).collect();
        let (unit, answer) = {
            let program = a.manifest.program().unwrap();
            let u = PublicUnitV1::KernelCommitments;
            (u, answer_artifact_unit_v1(&program, &a.leaves, &u).unwrap())
        };
        publish_artifact_v1(&refs[..2], &a.manifest, &a.leaves, &[(unit, answer)], 2).unwrap();
        // Provider 1 lies about leaf 3 (right length, other bytes); provider 2 holds nothing.
        let mut lie = a.leaves[3].clone();
        lie.bytes[0] ^= 0xFF;
        ps[1].put(&artifact_leaf_path(&a.manifest.artifact_root, 3), &borsh::to_vec(&lie).unwrap()).unwrap();
        let (net, class, root, kr) =
            (a.manifest.network_domain, a.manifest.v2_class(), a.manifest.artifact_root, a.manifest.kernel_param_root);
        let fetched =
            fetch_artifact_v1(&[refs[2], refs[1], refs[0]], net, class, root, Some(kr), Hash64::from_bytes([1; 64])).unwrap();
        assert_eq!(fetched.leaves, a.leaves, "the bytes are the class's, whoever served them");
        assert_eq!(check_binding_v1(&fetched, kr).unwrap(), BindingCheckV1::Confirmed);
        let unit =
            fetch_artifact_unit_v1(&refs, &fetched.facts(), &PublicUnitV1::KernelCommitments, Hash64::from_bytes([1; 64])).unwrap();
        assert_eq!(unit, PublicUnitAnswerV1::KernelCommitments { commitments: a.pc.clone() });
        // Wrong network / class / roots: no manifest is admissible.
        assert!(fetch_artifact_v1(&refs, Hash64::from_bytes([8; 64]), class, root, None, Hash64::default()).is_err());
        assert!(fetch_artifact_v1(&refs, net, Hash64::from_bytes([8; 64]), root, None, Hash64::default()).is_err());
        assert!(fetch_artifact_v1(&refs, net, class, root, Some(Hash64::from_bytes([8; 64])), Hash64::default()).is_err());

        // A FALSE pair: the class's bytes, another artifact's kernel root. The refuter fetches the true bytes, sees the roots differ,
        // forces the bound side's units out of the pair's provider (here, it serves them), and builds tag 105's proof.
        let other = art(12, None);
        let false_pair = art(11, Some(&other.pc));
        let (base2, ps2) = dirs("refute", 1);
        let refs2: Vec<&dyn MaterialProvider> = ps2.iter().map(|p| p as &dyn MaterialProvider).collect();
        let fprog = false_pair.manifest.program().unwrap();
        // The colluding provider's kernel-side units come from the BOUND bytes (the other artifact's).
        let bound_units: Vec<(PublicUnitV1, PublicUnitAnswerV1)> = {
            let mut v = vec![(
                PublicUnitV1::KernelCommitments,
                answer_artifact_unit_v1(&fprog, &other.leaves, &PublicUnitV1::KernelCommitments).unwrap(),
            )];
            for (param, layer) in differing_instances_v1(&a.pc, &other.pc) {
                let rows = misaka_palw_kernel::merkle::LayoutV1::of(&a.params.tensors[&(param, layer)].shape).rows;
                let u = PublicUnitV1::KernelRowNodes { param, layer, level: 0, first: 0, count: rows as u32 };
                v.push((u, answer_artifact_unit_v1(&fprog, &other.leaves, &u).unwrap()));
                for row in 0..rows {
                    let u = PublicUnitV1::KernelRow { param, layer, row };
                    v.push((u, answer_artifact_unit_v1(&fprog, &other.leaves, &u).unwrap()));
                }
            }
            v
        };
        publish_artifact_v1(&refs2, &false_pair.manifest, &false_pair.leaves, &bound_units, 1).unwrap();
        let kr2 = false_pair.manifest.kernel_param_root;
        let fetched = fetch_artifact_v1(&refs2, net, class, root, Some(kr2), Hash64::default()).unwrap();
        let BindingCheckV1::KernelRootDiffers { true_commitments } = check_binding_v1(&fetched, kr2).unwrap() else {
            panic!("refuted")
        };
        let facts = fetched.facts();
        let PublicUnitAnswerV1::KernelCommitments { commitments: bound } =
            fetch_artifact_unit_v1(&refs2, &facts, &PublicUnitV1::KernelCommitments, Hash64::default()).unwrap()
        else {
            unreachable!()
        };
        let (param, layer) = differing_instances_v1(&true_commitments, &bound)[0];
        let rows = misaka_palw_kernel::merkle::LayoutV1::of(&a.params.tensors[&(param, layer)].shape).rows;
        let PublicUnitAnswerV1::KernelRowNodes { run, .. } = fetch_artifact_unit_v1(
            &refs2,
            &facts,
            &PublicUnitV1::KernelRowNodes { param, layer, level: 0, first: 0, count: rows as u32 },
            Hash64::default(),
        )
        .unwrap() else {
            unreachable!()
        };
        let row = differing_rows_v1(&a.params.tensors[&(param, layer)], &run)[0];
        let PublicUnitAnswerV1::KernelRow { opening, .. } =
            fetch_artifact_unit_v1(&refs2, &facts, &PublicUnitV1::KernelRow { param, layer, row }, Hash64::default()).unwrap()
        else {
            unreachable!()
        };
        let proof = row_refutation_v1(&fetched.program, &fetched.leaves, root, &bound, param, layer, &opening).expect("a refutation");
        kaspa_consensus_core::palw_onboarding_v1::verify_artifact_mismatch_v1(&fetched.program, root, kr2, &proof).unwrap();
        let _ = std::fs::remove_dir_all(&base);
        let _ = std::fs::remove_dir_all(&base2);
    }

    /// Availability is per provider per leaf and local; a watcher repairs a thin provider from verified copies after the publisher is
    /// gone; the monitor is loud when nothing is left; the reference server refuses a manifest it cannot back; the hook writes a
    /// container whose tensors are the artifact's.
    #[test]
    fn availability_repair_retention_the_server_gate_and_the_root_fetch_container() {
        let a = art(5, None);
        let (base, ps) = dirs("repair", 3);
        let refs: Vec<&dyn MaterialProvider> = ps.iter().map(|p| p as &dyn MaterialProvider).collect();
        publish_artifact_v1(&refs[..1], &a.manifest, &a.leaves, &[], 1).unwrap();
        let program = a.manifest.program().unwrap();
        let av = artifact_availability_v1(&refs, &a.manifest, &program);
        assert_eq!((av[0].verified, av[1].verified, av[2].verified), (a.leaves.len(), 0, 0));
        assert_eq!(av[1].missing.len(), a.leaves.len());
        let mut monitor = PublicMaterialMonitorV1::new(2);
        monitor.watch(a.manifest.clone());
        let ev = monitor.tick(10, &refs, Hash64::default());
        assert_eq!(ev, vec![MaterialRetentionEventV1::Repaired { artifact_root: a.manifest.artifact_root, copies: 3 }]);
        assert!(matches!(monitor.tick(11, &refs, Hash64::default())[0], MaterialRetentionEventV1::Healthy { copies: 3, .. }));
        // Every copy gone: at risk, loudly. Past retention: expired.
        let _ = std::fs::remove_dir_all(&base);
        assert!(matches!(monitor.tick(12, &refs, Hash64::default())[0], MaterialRetentionEventV1::AtRisk { .. }));
        assert_eq!(
            monitor.tick(1_001, &refs, Hash64::default()),
            vec![MaterialRetentionEventV1::Expired { artifact_root: a.manifest.artifact_root }]
        );
        assert_eq!(monitor.watching(), 0);

        // The server gate: a manifest before its leaves is refused; after them stored.
        let srv = base.join("srv");
        std::fs::create_dir_all(&srv).unwrap();
        let mpath = artifact_manifest_path(&a.manifest.v2_class(), &a.manifest.artifact_root);
        let body = borsh::to_vec(&a.manifest).unwrap();
        assert!(server_accepts_material_v1(&srv, &mpath, &body).is_err());
        let fsrv = crate::evidence::fs::FsProvider::new(srv.clone());
        for (i, leaf) in a.leaves.iter().enumerate() {
            fsrv.put(&artifact_leaf_path(&a.manifest.artifact_root, i as u32), &borsh::to_vec(leaf).unwrap()).unwrap();
        }
        server_accepts_material_v1(&srv, &mpath, &body).unwrap();
        assert!(
            server_accepts_material_v1(&srv, &artifact_manifest_path(&Hash64::from_bytes([3; 64]), &a.manifest.artifact_root), &body)
                .is_err()
        );
        assert!(is_material_path(&mpath) && !is_material_path("artifacts/../etc/passwd") && !is_material_path("material/x/manifest"));

        // The root-fetch hook's container: its tensors are the artifact's.
        fsrv.put(&mpath, &body).unwrap();
        let fetched = fetch_artifact_v1(
            &[&fsrv],
            a.manifest.network_domain,
            a.manifest.v2_class(),
            a.manifest.artifact_root,
            None,
            Hash64::default(),
        )
        .unwrap();
        let out = base.join("hook.palwtir");
        write_container_v1(&out, &fetched).unwrap();
        let c = misaka_palw_tir_artifact::PalwTirContainerV1::open(&out).unwrap();
        for ((param, layer), t) in &a.params.tensors {
            assert_eq!(c.read_tensor_bytes(*param, *layer).unwrap(), t.to_le_bytes(), "{param} {layer:?}");
        }
        let _ = std::fs::remove_dir_all(&base);
    }
}
