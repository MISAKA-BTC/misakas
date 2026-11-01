//! Bounded local claims tape. The bytes are EXACT Borsh Vec<Vec<i128>> order,
//! suitable as the claims section of the existing local record hash recipe.
//! No new domain/ID, generic deserializer, signature or arithmetic certificate.
//! This tape detaches from producer model/run retention and implements the
//! bounded Core stream reader. Work/disk guards remain shared and cumulative.
//! Native transport/acceptance is not assigned by this local reader.
//! Existing Core record.hash's HashWriter recipe is preserved. The explicit
//! V2 stream path reads one claim at a time; the old Vec record remains a
//! compatibility API. Native body/version dispatch is not assigned here.
use super::proof::WidePanelBuilderV2;
use super::*;
/// Owns one empty local file. Not reopenable from a saved "complete" bit.
/// A callback/reader may retain exports only under its own additional reservation.
pub struct WideClaimsTapeV2 {
    tape: Tape,
    session: std::rc::Rc<Session>,
    cursor: u64,
    relations: u32,
    max_tile_lanes: u64,
}
impl WidePanelBuilderV2<'_, '_> {
    /// Per-tile temporaries stay under the live builder request. If callers
    /// materialize all claims (e.g. compatibility freeze_record), they MUST
    /// separately quote/acquire/retain the complete Vec capacities, binding,
    /// generated IDs, frozen record, signing/hash and Node-checker owners.
    /// retained_record_bytes!=0 is only a declared cap, never such a receipt.
    pub fn claims_tape(&self, file: File) -> Result<WideClaimsTapeV2, Error> {
        let count = self.plan().summary().relations;
        let relations = u32::try_from(count).map_err(|_| "wide claims Borsh count")?;
        let mut tape = Tape::empty(file)?;
        tape.append(&relations.to_le_bytes(), self.session())?;
        let mut max_tile_lanes = 0;
        for i in 0..count {
            let values = self.claim(i)?;
            let n = u32::try_from(values.len()).map_err(|_| "wide claims Borsh lane count")?;
            max_tile_lanes = max_tile_lanes.max(u64::from(n));
            tape.append(&n.to_le_bytes(), self.session())?;
            let mut chunk = [0u8; 4096];
            for values in values.chunks(chunk.len() / 16) {
                for (i, v) in values.iter().enumerate() {
                    chunk[i * 16..i * 16 + 16].copy_from_slice(&v.to_le_bytes());
                }
                tape.append(&chunk[..values.len() * 16], self.session())?;
            }
        }
        Ok(WideClaimsTapeV2 { tape, session: self.capture.session.clone(), cursor: 0, relations, max_tile_lanes })
    }
}
impl WideClaimsTapeV2 {
    pub fn encoded_bytes(&self) -> u64 {
        self.tape.len
    }
    pub fn relations(&self) -> u32 {
        self.relations
    }
    /// Stream the existing Borsh claims field, without materializing all claims.
    /// Writer scratch/retention belongs to its external owner; no live work reset.
    pub fn stream_borsh(&self, out: &mut dyn Write) -> Result<(), Error> {
        if self.tape.file.borrow().metadata().map_err(|e| e.to_string())?.len() != self.tape.len {
            return Err("wide claims tape length changed".into());
        }
        let mut at = 0;
        let mut b = [0; 4096];
        while at < self.tape.len {
            let n = (self.tape.len - at).min(b.len() as u64) as usize;
            self.tape.read(at, &mut b[..n], &self.session)?;
            self.session.charge(Charge::Io, n as u64)?;
            out.write_all(&b[..n]).map_err(|e| e.to_string())?;
            at += n as u64;
        }
        Ok(())
    }
    /// Sequential bounded access for a future stream-record checker. This emits
    /// raw Panel claims, never trusted operands or a stored checker result.
    /// The callback MUST acquire its own guard before retaining/copying values
    /// and keep it through that independent lifetime. A visit into Vec<Vec<_>>
    /// is the explicit RAM compatibility path, not stream-record completion.
    pub fn visit(&self, visit: &mut dyn FnMut(u32, &[i128]) -> Result<(), Error>) -> Result<(), Error> {
        let mut at = 0;
        let mut n = [0; 4];
        self.tape.read(at, &mut n, &self.session)?;
        at += 4;
        if u32::from_le_bytes(n) != self.relations {
            return Err("wide claims count changed".into());
        }
        for i in 0..self.relations {
            self.tape.read(at, &mut n, &self.session)?;
            at += 4;
            let n = u64::from(u32::from_le_bytes(n));
            if n > self.max_tile_lanes || mul(n, 16)? > self.session.limits.max_claim_bytes {
                return Err("wide claims tile changed".into());
            }
            let mut values = vec_exact(n as usize)?;
            let mut left = n;
            let mut chunk = [0u8; 4096];
            while left > 0 {
                let take = left.min((chunk.len() / 16) as u64) as usize;
                let bytes = take * 16;
                self.tape.read(at, &mut chunk[..bytes], &self.session)?;
                at = add(at, bytes as u64)?;
                for b in chunk[..bytes].chunks_exact(16) {
                    values.push(i128::from_le_bytes(b.try_into().map_err(|_| "wide claims lane")?));
                }
                left -= take as u64;
            }
            visit(i, &values)?;
        }
        if at != self.tape.len {
            return Err("wide claims trailing/missing data".into());
        }
        Ok(())
    }
}

impl WideClaimsTapeV2 {
    /// Reset only the transport cursor between producer hashing and an
    /// independent Node read. Work/IO/disk counters and guards never reset.
    pub fn rewind(&mut self) {
        self.cursor = 0;
    }
}
impl Read for WideClaimsTapeV2 {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let actual = self.tape.file.borrow().metadata()?.len();
        if actual != self.tape.len {
            return Err(std::io::Error::other("claims tape length changed"));
        }
        let n = usize::try_from((self.tape.len - self.cursor).min(buf.len() as u64))
            .map_err(|_| std::io::Error::other("claims tape read width"))?;
        if n == 0 {
            return Ok(0);
        }
        self.tape.read(self.cursor, &mut buf[..n], &self.session).map_err(std::io::Error::other)?;
        self.cursor += n as u64;
        Ok(n)
    }
}
impl kaspa_consensus_core::palw_positive_check_v2::full_relation::wide_v2::stream::ClaimsReaderV2 for WideClaimsTapeV2 {
    fn retained_request_bytes(&self) -> Option<u64> {
        Some(size_of::<Self>() as u64)
    }
    fn scratch_request_bytes(&self) -> Option<u64> {
        Some(0)
    }
}

impl WideClaimsTapeV2 {
    /// Producer hashing reads this tape once. The returned metadata/digest owns
    /// no model/run reference and certifies no arithmetic. Rewind only transport
    /// position for an INDEPENDENT Node read; cumulative IO never resets.
    #[allow(clippy::too_many_arguments)]
    pub fn freeze_record(
        &mut self,
        prep: &kaspa_consensus_core::palw_positive_check_v2::full_relation::wide_v2::FullRelationPreparationV2,
        binding: PalwTirStepBindingV1,
        generated: Vec<u32>,
        author: misaka_palw_positive_contract::identity::PrincipalId,
        limits: kaspa_consensus_core::palw_positive_check_v2::full_relation::wide_v2::stream::StreamRecordLimitsV2,
        reserver: &TirMemoryReserverV1,
    ) -> Result<kaspa_consensus_core::palw_positive_check_v2::full_relation::wide_v2::stream::StreamPanelRecordV2, Error> {
        let class = prep.job().shape_profile_id;
        let execution = Hash64::from_bytes(prep.result().execution.0);
        let acquire = |bytes| {
            reserver("positive-wide-record-stream-v2", class, execution, bytes)
                .map_err(|_| kaspa_consensus_core::palw_positive_check_v2::full_relation::FullRelationError::Budget)
        };
        prep.freeze_stream_record(binding, generated, author, self, limits, &acquire).map_err(|e| format!("wide record stream: {e:?}"))
    }
}
