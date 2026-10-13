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
    Collect { source_id: Hash64 },
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
        return Ok(FlatSourceScheduleV1::Collect { source_id: session.source_id });
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
    fn projected_path_is_literal_claim_scoped_not_private_state_scoped() {
        let claim = Hash64::from_u64_word(7);
        assert_eq!(path_v1(Path::new("retained"), claim), Path::new("retained").join(format!("{claim}.flat-public")));
        assert_ne!(path_v1(Path::new("retained"), claim), path_v1(Path::new("retained"), Hash64::from_u64_word(8)));
    }
}
