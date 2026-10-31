//! Producer-only original public Flat projection. These files contain no private checkpoint or
//! step tensor. Their native EOF hash, not their pathname/header, authorizes any verifier use.
use kaspa_consensus_core::{Hash64, palw_backend::PalwExecutionBackendV1};
use std::{
    fs::{File, OpenOptions},
    io::BufWriter,
    path::{Path, PathBuf},
};

pub(crate) enum FlatSourceScheduleV1 {
    Wait,
    Nominate { source_id: Hash64 },
    Ready { signer: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2 },
    Collect,
}

/// Consume only authenticated native session state; network/cache absence is never conviction.
/// If semantic work advances early, the native retained stage must still preserve every
/// original custodian's paid obligation until collection. No clock is rederived here.
pub(crate) fn request_v1(
    session: &kaspa_consensus_core::palw_flat_constraint_stream_v1::PalwFlatConstraintSourceSessionV1,
    now: u64,
) -> Result<FlatSourceScheduleV1, String> {
    if now < session.reserved_daa {
        return Err("Flat source snapshot precedes original paid reservation".into());
    }
    if !session.collected_v1() && now >= session.collection_due_daa_v1() && now < session.collect_until_daa {
        return Ok(FlatSourceScheduleV1::Collect);
    }
    if let Some(signer) = session.ready_signer_v1() {
        return Ok(FlatSourceScheduleV1::Ready { signer });
    }
    if now < session.nomination_until_daa && session.needs_nomination_v1() {
        return Ok(FlatSourceScheduleV1::Nominate { source_id: session.source_id });
    }
    Ok(FlatSourceScheduleV1::Wait)
}

/// Producer/explicit Valid-custodian response from its own original public sidecar or bounded
/// capture range. The caller holds host guards and retains this reader across bounded polls.
pub(crate) fn answer_v1(
    session: &kaspa_consensus_core::palw_flat_constraint_stream_v1::PalwFlatConstraintSourceSessionV1,
    signer: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
    now: u64,
    reader: &mut dyn kaspa_consensus_core::palw_backend::PalwFlatPublicSourceReaderV1,
) -> Result<Option<kaspa_consensus_core::palw_flat_constraint_stream_v1::PalwFlatConstraintSourceAnswerV1>, String> {
    use kaspa_consensus_core::palw_flat_constraint_stream_v1::{
        PalwFlatConstraintCursorV1 as Cursor, PalwFlatConstraintSourceAnswerV1,
    };
    if now < session.reserved_daa || session.response_deadline_v1(&signer).is_none_or(|until| now >= until) || session.collected_v1() {
        return Ok(None);
    }
    let cursor = session.slot_cursor_v1(&signer)?;
    if matches!(cursor, Cursor::Complete | Cursor::Nominate) {
        return Ok(None);
    }
    let Some(action) = reader.poll_v1(cursor)? else {
        return Ok(None);
    };
    action.check_wire_v1()?;
    Ok(Some(PalwFlatConstraintSourceAnswerV1 { answerer: signer, source_id: session.source_id, cursor, action }))
}

pub(crate) fn path_v1(dir: &Path, claim: Hash64) -> PathBuf {
    dir.join(format!("{claim}.flat-public"))
}

/// The claim's generic public Float32 sidecar (`palw_float_constraint_source_v1`'s format).
pub(crate) fn float_path_v1(dir: &Path, claim: Hash64) -> PathBuf {
    dir.join(format!("{claim}.float-public"))
}

pub(crate) enum FloatSourceScheduleV1 {
    Wait,
    Nominate { source_id: Hash64 },
    Ready { signer: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2 },
    Collect,
}

/// The greedy Float32 owner's next move, from authenticated native session state only (the
/// Flat schedule's rule: collection first, then a ready slot, then nomination).
pub(crate) fn float_request_v1(
    session: &kaspa_consensus_core::palw_float_constraint_source_v1::PalwFloatConstraintSourceSessionV1,
    now: u64,
) -> Result<FloatSourceScheduleV1, String> {
    if now < session.reserved_daa {
        return Err("Float32 source snapshot precedes original paid reservation".into());
    }
    if !session.collected_v1() && now >= session.collection_due_daa_v1() && now < session.collect_until_daa {
        return Ok(FloatSourceScheduleV1::Collect);
    }
    if let Some(signer) = session.ready_signer_v1() {
        return Ok(FloatSourceScheduleV1::Ready { signer });
    }
    if now < session.nomination_until_daa && session.needs_nomination_v1() {
        return Ok(FloatSourceScheduleV1::Nominate { source_id: session.source_id });
    }
    Ok(FloatSourceScheduleV1::Wait)
}

/// A custodian's next Float32 answer from its own public reader (sidecar or backend projection).
pub(crate) fn float_answer_v1(
    session: &kaspa_consensus_core::palw_float_constraint_source_v1::PalwFloatConstraintSourceSessionV1,
    signer: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
    now: u64,
    reader: &mut dyn kaspa_consensus_core::palw_backend::PalwFloatPublicSourceReaderV1,
) -> Result<Option<kaspa_consensus_core::palw_float_constraint_source_v1::PalwFloatSourceAnswerV1>, String> {
    use kaspa_consensus_core::palw_float_constraint_source_v1::{PalwFloatSourceAnswerV1, PalwFloatSourceCursorV1 as Cursor};
    if now < session.reserved_daa || session.response_deadline_v1(&signer).is_none_or(|until| now >= until) || session.collected_v1() {
        return Ok(None);
    }
    let cursor = session.slot_cursor_v1(&signer)?;
    if matches!(cursor, Cursor::Complete | Cursor::Nominate) {
        return Ok(None);
    }
    let Some(action) = reader.poll_v1(cursor, session.nomination_v1().map(|(position, _)| position))? else {
        return Ok(None);
    };
    action.check_wire_v1()?;
    Ok(Some(PalwFloatSourceAnswerV1 { answerer: signer, source_id: session.source_id, cursor, action }))
}

/// The Float32 twin of [`retain_v1`]: the backend projects its run's public f32 rows into the
/// generic sidecar, or answers `false` (it retains none — every in-node backend today).
pub(crate) fn retain_float_v1(
    dir: &Path,
    claim: Hash64,
    backend: &dyn PalwExecutionBackendV1,
    capture: &[u8],
) -> Result<bool, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let path = float_path_v1(dir, claim);
    let partial = path.with_extension("float-public.partial");
    let mut cleanup = PartialV1(Some(partial.clone()));
    let file = OpenOptions::new().write(true).create(true).truncate(true).open(&partial).map_err(|e| e.to_string())?;
    let mut writer = BufWriter::with_capacity(16_384, file);
    if !backend.write_float_public_source_v1(capture, &mut writer)? {
        return Ok(false);
    }
    let file = writer.into_inner().map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    std::fs::rename(&partial, &path).map_err(|e| e.to_string())?;
    cleanup.0 = None;
    File::open(dir).and_then(|dir| dir.sync_all()).map_err(|e| e.to_string())?;
    Ok(true)
}

/// Call off the async runtime under the producer's already-held FullSeat reservation, before
/// publishing the original claim. The backend projects from that run's capture, never executes.
pub(crate) fn retain_v1(dir: &Path, claim: Hash64, backend: &dyn PalwExecutionBackendV1, capture: &[u8]) -> Result<bool, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let path = path_v1(dir, claim);
    let partial = path.with_extension("flat-public.partial");
    let mut cleanup = PartialV1(Some(partial.clone()));
    let file = OpenOptions::new().write(true).create(true).truncate(true).open(&partial).map_err(|e| e.to_string())?;
    let mut writer = BufWriter::with_capacity(16_384, file);
    if !backend.write_flat_public_source_v1(capture, &mut writer)? {
        return Ok(false);
    }
    let file = writer.into_inner().map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    std::fs::rename(&partial, &path).map_err(|e| e.to_string())?;
    cleanup.0 = None;
    File::open(dir).and_then(|dir| dir.sync_all()).map_err(|e| e.to_string())?;
    Ok(true)
}
/// Remove one claim's shared original public projection. The native caller supplies the exact
/// accepted claim-wide union and the irreversible pruning clock; a path or mtime never does.
/// The store freezes such an expired union, so no later accepted lease can resurrect it.
pub(crate) fn gc_shared_claim_v1(dir: &Path, claim: Hash64, material_until: u64, irreversible_daa: u64) -> Result<bool, String> {
    if irreversible_daa <= material_until {
        return Ok(false);
    }
    let path = path_v1(dir, claim);
    let float = float_path_v1(dir, claim);
    for victim in [path.with_extension("flat-public.partial"), path, float.with_extension("float-public.partial"), float] {
        match std::fs::remove_file(&victim) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("shared public projection {}: {e}", victim.display())),
        }
    }
    match File::open(dir).and_then(|dir| dir.sync_all()) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(e) => Err(e.to_string()),
    }
}

/// Service-wide, keyless retirement of claim-scoped shared public material. One native page of
/// at most 64 expiry rows is read at a time; metadata is acknowledged only after the files are
/// gone, and a refused ACK (a changed union) restarts the bounded scan.
#[derive(Default)]
pub(crate) struct SharedClaimCacheGcV1 {
    after: Option<(u64, Hash64)>,
    irreversible_daa: u64,
    queued: std::collections::VecDeque<kaspa_consensus_core::palw_prefix_history_v1::PalwSharedClaimCacheLeaseV1>,
}

impl SharedClaimCacheGcV1 {
    pub(crate) async fn tick_v1(&mut self, session: &kaspa_consensusmanager::ConsensusProxy, directory: &Path) -> Result<(), String> {
        if self.queued.is_empty() {
            let after = self.after;
            let page = session.clone().spawn_blocking(move |c| c.palw_shared_claim_cache_gc_page_v1(after, 64)).await?;
            if page.leases.len() > 64 {
                return Err("native shared cache GC metadata exceeds bounded page".into());
            }
            self.after = page.next;
            self.irreversible_daa = page.irreversible_daa;
            self.queued.extend(page.leases);
        }
        let Some(lease) = self.queued.pop_front() else { return Ok(()) };
        if self.irreversible_daa <= lease.material_until_daa {
            return Ok(());
        }
        let directory = directory.to_path_buf();
        let irreversible = self.irreversible_daa;
        let done =
            tokio::task::spawn_blocking(move || gc_shared_claim_v1(&directory, lease.claim, lease.material_until_daa, irreversible))
                .await
                .map_err(|e| format!("bounded shared cache GC worker failed: {e}"))??;
        if done {
            let accepted = session.clone().spawn_blocking(move |c| c.palw_shared_claim_cache_gc_ack_v1(lease)).await?;
            if !accepted {
                self.after = None;
            }
        }
        Ok(())
    }
}

/// A producer's projection written before its carrier is built. Dropped without `keep`, the
/// files are removed: no accepted claim exists, so no native lease would ever retire them.
/// Only a fresh claim (the canonical builder's new random job nonce) may own this guard.
pub(crate) struct UnpublishedProjectionV1(Option<(PathBuf, PathBuf)>);
impl UnpublishedProjectionV1 {
    pub(crate) fn new(dir: &Path, claim: Hash64) -> Self {
        Self(Some((path_v1(dir, claim), float_path_v1(dir, claim))))
    }
    pub(crate) fn keep(&mut self) {
        self.0 = None;
    }
}
impl Drop for UnpublishedProjectionV1 {
    fn drop(&mut self) {
        if let Some((path, float)) = self.0.take() {
            let _ = std::fs::remove_file(path.with_extension("flat-public.partial"));
            let _ = std::fs::remove_file(path);
            let _ = std::fs::remove_file(float.with_extension("float-public.partial"));
            let _ = std::fs::remove_file(float);
        }
    }
}

struct PartialV1(Option<PathBuf>);
impl Drop for PartialV1 {
    fn drop(&mut self) {
        if let Some(path) = self.0.take() {
            let _ = std::fs::remove_file(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_projection_gc_needs_strictly_irreversible_expiry_and_is_idempotent() {
        let directory = tempfile::tempdir().unwrap();
        let claim = Hash64::from_u64_word(7);
        let other = Hash64::from_u64_word(8);
        std::fs::write(path_v1(directory.path(), claim), b"projection").unwrap();
        std::fs::write(path_v1(directory.path(), claim).with_extension("flat-public.partial"), b"partial").unwrap();
        std::fs::write(path_v1(directory.path(), other), b"another claim").unwrap();
        assert!(!gc_shared_claim_v1(directory.path(), claim, 200, 200).unwrap());
        assert!(path_v1(directory.path(), claim).exists());
        assert!(gc_shared_claim_v1(directory.path(), claim, 200, 201).unwrap());
        assert!(!path_v1(directory.path(), claim).exists());
        assert!(!path_v1(directory.path(), claim).with_extension("flat-public.partial").exists());
        assert!(path_v1(directory.path(), other).exists(), "only the literal expired claim is retired");
        assert!(gc_shared_claim_v1(directory.path(), claim, 200, 201).unwrap(), "a repeated GC after a lost ACK is a no-op");
    }
    #[test]
    fn an_unbuilt_carrier_leaves_no_orphan_projection_but_a_built_one_keeps_it() {
        let directory = tempfile::tempdir().unwrap();
        let (refused, built) = (Hash64::from_u64_word(9), Hash64::from_u64_word(10));
        let unbuilt = UnpublishedProjectionV1::new(directory.path(), refused);
        let mut kept = UnpublishedProjectionV1::new(directory.path(), built);
        std::fs::write(path_v1(directory.path(), refused), b"projection").unwrap();
        std::fs::write(path_v1(directory.path(), built), b"projection").unwrap();
        drop(unbuilt);
        kept.keep();
        drop(kept);
        assert!(!path_v1(directory.path(), refused).exists());
        assert!(path_v1(directory.path(), built).exists());
    }
    #[test]
    fn projection_failure_removes_both_formats_and_partial_writes() {
        let directory = tempfile::tempdir().unwrap();
        let claim = Hash64::from_u64_word(11);
        let flat = path_v1(directory.path(), claim);
        let float = float_path_v1(directory.path(), claim);
        for failed_after in 1..=4 {
            let result = (|| -> Result<(), String> {
                let _projection = UnpublishedProjectionV1::new(directory.path(), claim);
                for (index, path) in
                    [&flat, &flat.with_extension("flat-public.partial"), &float, &float.with_extension("float-public.partial")]
                        .into_iter()
                        .enumerate()
                {
                    std::fs::write(path, b"unfinished projection").unwrap();
                    if index + 1 == failed_after {
                        return Err("backend projection or header refused".into());
                    }
                }
                Ok(())
            })();
            assert!(result.is_err());
            for path in [&flat, &flat.with_extension("flat-public.partial"), &float, &float.with_extension("float-public.partial")] {
                assert!(!path.exists(), "a failed publication cannot leave {} without a native lease", path.display());
            }
        }
    }
    #[test]
    fn projected_path_is_literal_claim_scoped_not_private_state_scoped() {
        let claim = Hash64::from_u64_word(7);
        assert_eq!(path_v1(Path::new("retained"), claim), Path::new("retained").join(format!("{claim}.flat-public")));
        assert_ne!(path_v1(Path::new("retained"), claim), path_v1(Path::new("retained"), Hash64::from_u64_word(8)));
    }
}
