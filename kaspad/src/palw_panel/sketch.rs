//! **RFC-0007 Part II, the node's side of the seat-local algebraic checker** (node policy; a child of `palw_panel`). Nothing here is a
//! consensus rule: the chain pins how many trace chunks an attempt of a class commits (`palw_witness_manifest_v1`) and lets an
//! `Unavailable` verdict name one; whether a seat checks a witness algebraically, replays, or both is the seat's own. Behind
//! `--palw-sketch-check`, **off by default**, and a node without the flag never opens this module's state.
//!
//! * **The seat's secret** ([`palw_sketch_secret_load_v1`]): 32 bytes drawn from the operating system once and kept in the state dir,
//!   mode 0600, never sent anywhere. Every sketch vector derives from it, per class and epoch; a sketch is as secret as the secret
//!   (RFC-0007 §II.4), so nothing here is serialised, logged or put in a status line.
//! * **The sketches** ([`PalwSketchServiceV1`]): one store per `(class, epoch)`, built in one pass over the held artifact and kept
//!   until the epoch turns.
//! * **The mirror check** ([`PalwSketchServiceV1::mirror_replay_v1`]): run **beside the full replay on every claim the seat replays**.
//!   The seat produces the claim's witness itself on the typed backend (honest by construction), checks it algebraically, and compares
//!   the checker's verdict with the replay's; a disagreement is logged at `error` and counted (`sketch_mirror_false_rejects`,
//!   `sketch_mirror_false_accepts`), and the status line carries the totals. Until the mirror has run clean over real claims no seat
//!   should rely on the checker alone. (A producer-served witness takes the same path through
//!   [`PalwSketchServiceV1::mirror_served_v1`]; the transport that carries one is the interval lane's, RFC-0007 §II.7.)

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_tir_artifact_v1::PalwTirTensorSourceV1;
use kaspa_core::{error, info, warn};
use misaka_palw_sdk::lineage::PalwTirClassEntryV1;
use misaka_palw_tir::{ParamSource, Tensor};
use misaka_palw_tir_sketch::{
    TirCheckPolicyV1, TirMirrorAgreementV1, TirMirrorTotalsV1, TirSeatSketchSecretV1, TirSketchAnalysisV1, TirSketchJobV1, TirSketchKeysV1,
    TirSketchStoreV1, TirWitnessV1, tir_mirror_agreement_v1, tir_mirror_check_v1, tir_witness_capture_v1,
};

use super::PALW_PANEL;

/// How long a sketch store lives: the epoch's length in DAA. A store is rebuilt when the epoch turns (RFC-0007 §II.4: a refresh bounds
/// what one probe can learn).
pub(crate) const PALW_SKETCH_EPOCH_DAA_V1: u64 = 7_200;

/// The state-dir file the seat's sketch secret lives in.
pub(crate) const PALW_SKETCH_SECRET_FILE_V1: &str = "palw-sketch-secret";

/// **Load the seat's sketch secret, or draw one**: a 32-byte file in `dir`, created mode 0600 if absent. An unreadable or wrongly sized
/// file is an error, never a silent redraw (a redraw would orphan every store built under the old secret and tell the operator nothing).
pub(crate) fn palw_sketch_secret_load_v1(dir: &Path) -> Result<TirSeatSketchSecretV1, String> {
    let path = dir.join(PALW_SKETCH_SECRET_FILE_V1);
    match std::fs::read(&path) {
        Ok(bytes) => {
            let bytes: [u8; 32] = bytes
                .as_slice()
                .try_into()
                .map_err(|_| format!("{} is {} bytes, not the 32 a sketch secret is", path.display(), bytes.len()))?;
            Ok(TirSeatSketchSecretV1::from_bytes(bytes))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
            // The type has no accessor for its bytes by design, so the file is written from the draw and the secret built from it:
            // one source of truth, never two draws.
            let mut bytes = [0u8; 32];
            use rand::RngCore;
            rand::rngs::OsRng.fill_bytes(&mut bytes);
            write_private(&path, &bytes)?;
            Ok(TirSeatSketchSecretV1::from_bytes(bytes))
        }
        Err(e) => Err(format!("cannot read {}: {e}", path.display())),
    }
}

#[cfg(unix)]
fn write_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|e| format!("cannot create {}: {e}", path.display()))?;
    file.write_all(bytes).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

#[cfg(not(unix))]
fn write_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    std::fs::write(path, bytes).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

/// A held artifact as the interpreter's `ParamSource` (an owned tensor per request).
struct ArtifactParamsV1<'a> {
    artifact: &'a misaka_palw_tir_exec::node::TirArtifactV1,
    program: &'a misaka_palw_tir::TirProgramV1,
}

impl ParamSource for ArtifactParamsV1<'_> {
    fn param(&self, index: u16, layer: Option<u16>) -> Option<Tensor> {
        let bytes = self.artifact.tensor_bytes(index, layer)?;
        let decl = self.program.params.get(index as usize)?;
        let shape: Vec<usize> = decl.shape.iter().map(|d| *d as usize).collect();
        Tensor::from_le_bytes(decl.dtype, &shape, &bytes).ok()
    }
}

/// One class's sketches in one epoch.
struct StoreV1 {
    analysis: TirSketchAnalysisV1,
    keys: TirSketchKeysV1,
    store: TirSketchStoreV1,
}

/// **The sketch service**: the seat's secret, the per-`(class, epoch)` stores, the check policy and the mirror's totals.
pub(crate) struct PalwSketchServiceV1 {
    secret: TirSeatSketchSecretV1,
    policy: TirCheckPolicyV1,
    stores: Mutex<HashMap<(Hash64, u64), Arc<StoreV1>>>,
    totals: Mutex<TirMirrorTotalsV1>,
}

impl PalwSketchServiceV1 {
    /// Open the service in `state_dir` (the secret is loaded or drawn there).
    pub(crate) fn open(state_dir: &Path, policy: TirCheckPolicyV1) -> Result<Self, String> {
        let secret = palw_sketch_secret_load_v1(state_dir)?;
        Ok(Self { secret, policy, stores: Mutex::new(HashMap::new()), totals: Mutex::new(TirMirrorTotalsV1::default()) })
    }

    /// A service with a given secret and policy, for tests.
    #[cfg(test)]
    pub(crate) fn with_secret(secret: [u8; 32], policy: TirCheckPolicyV1) -> Self {
        Self {
            secret: TirSeatSketchSecretV1::from_bytes(secret),
            policy,
            stores: Mutex::new(HashMap::new()),
            totals: Mutex::new(TirMirrorTotalsV1::default()),
        }
    }

    /// The mirror's totals, as the status line's pairs.
    pub(crate) fn status_v1(&self) -> String {
        self.totals.lock().expect("the totals are never poisoned").status()
    }

    #[cfg(test)]
    pub(crate) fn totals_v1(&self) -> TirMirrorTotalsV1 {
        *self.totals.lock().expect("the totals are never poisoned")
    }

    /// The store of `entry`'s class in `epoch`, built on first use (a pass over the whole artifact) and dropped with its epoch.
    fn store_v1(&self, entry: &PalwTirClassEntryV1, epoch: u64) -> Result<Arc<StoreV1>, String> {
        let class_id = entry.class_id();
        let key = (class_id, epoch);
        if let Some(held) = self.stores.lock().expect("the stores are never poisoned").get(&key) {
            return Ok(held.clone());
        }
        let plan = entry.artifact.plan();
        let analysis = TirSketchAnalysisV1::of(&plan.program);
        let mut class_bytes = [0u8; 64];
        class_bytes.copy_from_slice(class_id.as_byte_slice());
        let keys = self.secret.keys(&class_bytes, epoch);
        let params = ArtifactParamsV1 { artifact: &entry.artifact, program: &plan.program };
        let started = std::time::Instant::now();
        let store = TirSketchStoreV1::build(plan, &analysis, &params, &keys).map_err(|e| format!("the sketch store does not build: {e}"))?;
        info!(
            "[{PALW_PANEL}] sketch store of class {class_id} for epoch {epoch} built in {} ms ({:?})",
            started.elapsed().as_millis(),
            store.stats()
        );
        let built = Arc::new(StoreV1 { analysis, keys, store });
        let mut stores = self.stores.lock().expect("the stores are never poisoned");
        // A store of an older epoch of this class is dropped (and wiped with it).
        stores.retain(|(class, at), _| *class != class_id || *at >= epoch);
        stores.insert(key, built.clone());
        Ok(built)
    }

    fn job_id_v1(claim: &Hash64, epoch: u64) -> [u8; 32] {
        let mut h = blake2b_simd::Params::new().hash_length(32).key(b"misaka-node/sketch-mirror-job/v1").to_state();
        h.update(claim.as_byte_slice());
        h.update(&epoch.to_le_bytes());
        let mut out = [0u8; 32];
        out.copy_from_slice(h.finalize().as_bytes());
        out
    }

    /// **The mirror check on the seat's own witness** — blocking (a witness is a second execution of the job, a check a pass over its
    /// values). `replay_reproduces` is the replay's verdict on the claim. Returns how the two compare; an alarm is logged here.
    pub(crate) fn mirror_replay_v1(
        &self,
        entry: &PalwTirClassEntryV1,
        claim: Hash64,
        prompt: &[usize],
        decode_tokens: u32,
        replay_reproduces: bool,
        daa: u64,
    ) -> Result<TirMirrorAgreementV1, String> {
        let epoch = daa / PALW_SKETCH_EPOCH_DAA_V1;
        let held = self.store_v1(entry, epoch)?;
        let job = TirSketchJobV1 { prompt: prompt.iter().map(|t| *t as u32).collect(), decode: decode_tokens };
        let plan = entry.artifact.plan();
        let witness = tir_witness_capture_v1(plan, &held.analysis, entry.artifact.params(), &job, &self.policy)
            .map_err(|e| format!("the witness does not capture: {e}"))?;
        self.check_witness_v1(entry, &held, claim, &job, &witness, false, replay_reproduces, epoch)
    }

    /// **The mirror check on a witness the producer served** (the same comparison; a refusal of a served witness of a claim the replay
    /// reproduces is no disagreement, a served witness of a claim it does not reproduce that the checker ACCEPTS is the alarm).
    #[allow(clippy::too_many_arguments, dead_code)] // the transport that carries a served witness is the interval lane's (RFC-0007 §II.7)
    pub(crate) fn mirror_served_v1(
        &self,
        entry: &PalwTirClassEntryV1,
        claim: Hash64,
        prompt: &[usize],
        decode_tokens: u32,
        served: &TirWitnessV1,
        replay_reproduces: bool,
        daa: u64,
    ) -> Result<TirMirrorAgreementV1, String> {
        let epoch = daa / PALW_SKETCH_EPOCH_DAA_V1;
        let held = self.store_v1(entry, epoch)?;
        let job = TirSketchJobV1 { prompt: prompt.iter().map(|t| *t as u32).collect(), decode: decode_tokens };
        self.check_witness_v1(entry, &held, claim, &job, served, true, replay_reproduces, epoch)
    }

    #[allow(clippy::too_many_arguments)]
    fn check_witness_v1(
        &self,
        entry: &PalwTirClassEntryV1,
        held: &StoreV1,
        claim: Hash64,
        job: &TirSketchJobV1,
        witness: &TirWitnessV1,
        served: bool,
        replay_reproduces: bool,
        epoch: u64,
    ) -> Result<TirMirrorAgreementV1, String> {
        let plan = entry.artifact.plan();
        let params = ArtifactParamsV1 { artifact: &entry.artifact, program: &plan.program };
        let started = std::time::Instant::now();
        let outcome = tir_mirror_check_v1(
            plan,
            &held.analysis,
            &held.store,
            &held.keys,
            &params,
            job,
            &Self::job_id_v1(&claim, epoch),
            witness,
            self.policy,
        )
        .map_err(|e| {
            self.totals.lock().expect("the totals are never poisoned").errors += 1;
            format!("the sketch check does not run: {e}")
        })?;
        let agreement = tir_mirror_agreement_v1(served, replay_reproduces, outcome.accepted);
        self.totals.lock().expect("the totals are never poisoned").note(agreement, outcome.served_bytes);
        if agreement.is_alarm() {
            error!(
                "[{PALW_PANEL}] SKETCH MIRROR DISAGREES with the replay on claim {claim} ({agreement:?}: {} witness, replay {}, checker {}): {:?} — \
                 do not rely on the sketch checker until this is understood (RFC-0007 Part II)",
                if served { "a served" } else { "this seat's own" },
                if replay_reproduces { "reproduces the claim" } else { "does not reproduce it" },
                if outcome.accepted { "accepts" } else { "refuses" },
                outcome.failure
            );
        } else if matches!(agreement, TirMirrorAgreementV1::WitnessRefused) {
            warn!(
                "[{PALW_PANEL}] claim {claim}: the served witness was refused by the sketch check ({:?}) while the replay reproduces the \
                 claim — the witness is wrong, not the claim",
                outcome.failure
            );
        } else {
            info!(
                "[{PALW_PANEL}] claim {claim}: the sketch check agrees with the replay ({} bytes served, {} ms)",
                outcome.served_bytes,
                started.elapsed().as_millis()
            );
        }
        Ok(agreement)
    }
}

/// **A claim's witness, as the producer serves it** (RFC-0007 Part II §II.2): the canonical image of the witness of the job the claim
/// answers, produced on the typed backend under the canonical serving set. Blocking — it is a second execution of the job.
pub(crate) fn palw_witness_image_v1(entry: &PalwTirClassEntryV1, prompt: &[usize], decode_tokens: u32) -> Result<Vec<u8>, String> {
    let plan = entry.artifact.plan();
    let analysis = TirSketchAnalysisV1::of(&plan.program);
    let job = TirSketchJobV1 { prompt: prompt.iter().map(|t| *t as u32).collect(), decode: decode_tokens };
    let witness = tir_witness_capture_v1(plan, &analysis, entry.artifact.params(), &job, &TirCheckPolicyV1::default())
        .map_err(|e| format!("the witness does not capture: {e}"))?;
    Ok(misaka_palw_tir_sketch::codec::encode(&witness))
}

/// The retention file of a claim's witness image.
pub(crate) fn palw_witness_path_v1(retention_dir: &Path, claim: &Hash64) -> std::path::PathBuf {
    retention_dir.join(format!("{claim}.witness"))
}

/// The retention file of a claim's witness chunk digests: a `u32` count and one 32-byte digest per chunk, in chunk order.
pub(crate) fn palw_witness_chunks_path_v1(retention_dir: &Path, claim: &Hash64) -> std::path::PathBuf {
    retention_dir.join(format!("{claim}.witness.chunks"))
}

/// **Keep a claim's witness** as the producer's obligation (RFC-0007 §II.7): the image, and the digests of the exactly-`chunks` pieces it
/// is cut into (the count the chain pinned the attempt at), each file written beside and renamed so a reader never sees half of one.
/// Returns the digests.
pub(crate) fn palw_witness_retain_v1(retention_dir: &Path, claim: &Hash64, image: &[u8], chunks: u32) -> Result<Vec<[u8; 32]>, String> {
    std::fs::create_dir_all(retention_dir).map_err(|e| format!("cannot create {}: {e}", retention_dir.display()))?;
    let pieces = misaka_palw_tir_sketch::codec::split_into(image, chunks as usize);
    let digests: Vec<[u8; 32]> =
        pieces.iter().enumerate().map(|(i, piece)| misaka_palw_tir_sketch::codec::chunk_digest(i as u32, piece)).collect();
    let mut manifest = Vec::with_capacity(4 + digests.len() * 32);
    manifest.extend_from_slice(&chunks.to_le_bytes());
    for digest in &digests {
        manifest.extend_from_slice(digest);
    }
    let put = |path: std::path::PathBuf, bytes: &[u8]| -> Result<(), String> {
        let tmp = path.with_extension("partial");
        std::fs::write(&tmp, bytes).and_then(|()| std::fs::rename(&tmp, &path)).map_err(|e| format!("cannot write {}: {e}", path.display()))
    };
    put(palw_witness_path_v1(retention_dir, claim), image)?;
    put(palw_witness_chunks_path_v1(retention_dir, claim), &manifest)?;
    Ok(digests)
}

/// **Serve one witness chunk of a retained claim** (`index` 0-based within the witness; the manifest's chunk `1 + index`): the bytes, or
/// `None` where the claim's witness is not kept here, the index is past its chunks, or the retained piece no longer matches its digest.
#[allow(dead_code)] // served by the interval lane's transport once it exists (RFC-0007 §II.7); the tests read it today
pub(crate) fn palw_witness_chunk_v1(retention_dir: &Path, claim: &Hash64, index: u32) -> Option<Vec<u8>> {
    let manifest = std::fs::read(palw_witness_chunks_path_v1(retention_dir, claim)).ok()?;
    let count = u32::from_le_bytes(manifest.get(..4)?.try_into().ok()?);
    if index >= count || manifest.len() != 4 + count as usize * 32 {
        return None;
    }
    let digest: [u8; 32] = manifest.get(4 + index as usize * 32..4 + (index as usize + 1) * 32)?.try_into().ok()?;
    let image = std::fs::read(palw_witness_path_v1(retention_dir, claim)).ok()?;
    let piece = misaka_palw_tir_sketch::codec::split_into(&image, count as usize).get(index as usize)?.to_vec();
    (misaka_palw_tir_sketch::codec::chunk_digest(index, &piece) == digest).then_some(piece)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_retained_witness_serves_its_chunks_and_only_true_ones() {
        let dir = tempfile::tempdir().unwrap();
        let claim = Hash64::from_bytes([4; 64]);
        let image: Vec<u8> = (0..5_000u32).flat_map(|i| i.to_le_bytes()).collect();
        let digests = palw_witness_retain_v1(dir.path(), &claim, &image, 4).expect("kept");
        assert_eq!(digests.len(), 4);
        let served: Vec<(u32, Vec<u8>)> = (0..4).map(|i| (i, palw_witness_chunk_v1(dir.path(), &claim, i).expect("a chunk"))).collect();
        assert_eq!(misaka_palw_tir_sketch::codec::assemble(served, &digests), Some(image.clone()), "the chunks are the image");
        assert!(palw_witness_chunk_v1(dir.path(), &claim, 4).is_none(), "past the pinned count");
        assert!(palw_witness_chunk_v1(dir.path(), &Hash64::from_bytes([5; 64]), 0).is_none(), "another claim's");
        // A retained piece that no longer matches its digest is not served.
        let mut damaged = image.clone();
        damaged[0] ^= 1;
        std::fs::write(palw_witness_path_v1(dir.path(), &claim), &damaged).unwrap();
        assert!(palw_witness_chunk_v1(dir.path(), &claim, 0).is_none(), "a damaged chunk is not served");
        assert!(palw_witness_chunk_v1(dir.path(), &claim, 3).is_some(), "a chunk the damage did not touch still is");
    }

    #[test]
    fn the_secret_is_drawn_once_kept_private_and_never_redrawn_silently() {
        let dir = tempfile::tempdir().unwrap();
        let first = palw_sketch_secret_load_v1(dir.path()).expect("drawn");
        let again = palw_sketch_secret_load_v1(dir.path()).expect("loaded");
        let class = [3u8; 64];
        let m = misaka_palw_tir_sketch::TirSketchModulusV1::P61;
        assert_eq!(first.keys(&class, 1).site_vector(0, 0, m, 8), again.keys(&class, 1).site_vector(0, 0, m, 8), "the same secret both times");
        let path = dir.path().join(PALW_SKETCH_SECRET_FILE_V1);
        assert_eq!(std::fs::read(&path).unwrap().len(), 32);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600, "mode 0600");
        }
        // A file that is not 32 bytes is an error, and is left as it is.
        std::fs::write(&path, b"short").unwrap();
        let why = palw_sketch_secret_load_v1(dir.path()).err().expect("refused");
        assert!(why.contains("5 bytes") && why.contains("32"), "{why}");
        assert_eq!(std::fs::read(&path).unwrap(), b"short", "not overwritten");
        // Another directory, another secret.
        let other = tempfile::tempdir().unwrap();
        let other_secret = palw_sketch_secret_load_v1(other.path()).unwrap();
        assert_ne!(first.keys(&class, 1).site_vector(0, 0, m, 8), other_secret.keys(&class, 1).site_vector(0, 0, m, 8));
    }

    #[test]
    fn the_status_names_the_totals_and_no_secret() {
        let service = PalwSketchServiceV1::with_secret([0xAB; 32], TirCheckPolicyV1::default());
        let line = service.status_v1();
        assert!(line.contains("sketch_mirror_checked=0") && line.contains("sketch_mirror_false_accepts=0"), "{line}");
    }
}
