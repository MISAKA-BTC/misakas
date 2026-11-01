//! Bounded local claims tape. The bytes are EXACT Borsh Vec<Vec<i128>> order,
//! suitable as the claims section of the existing local record hash recipe.
//! No new domain/ID, generic deserializer, signature or arithmetic certificate.
//! The current Node PrototypePanelRecord still owns Vec<Vec<i128>>: stream-backed
//! record signing/checking is a required next integration, not completed here.
//! Existing Core record.hash already uses HashWriter streaming: the missing
//! part is replacing its retained Vec<Vec<i128>> owner/reader, not inventing a
//! new hash/domain or an extra full encoded-record allocation.
use super::proof::WidePanelBuilderV2;
use super::*;
/// Owns one empty local file. Not reopenable from a saved "complete" bit.
/// A callback/reader may retain exports only under its own additional reservation.
pub struct WideClaimsTapeV2<'s> {
    tape: Tape,
    session: &'s Session,
    relations: u32,
    max_tile_lanes: u64,
}
impl WidePanelBuilderV2<'_, '_> {
    /// Per-tile temporaries stay under the live builder request. If callers
    /// materialize all claims (e.g. compatibility freeze_record), they MUST
    /// separately quote/acquire/retain the complete Vec capacities, binding,
    /// generated IDs, frozen record, signing/hash and Node-checker owners.
    /// retained_record_bytes!=0 is only a declared cap, never such a receipt.
    pub fn claims_tape<'s>(&'s self, file: File) -> Result<WideClaimsTapeV2<'s>, Error> {
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
        Ok(WideClaimsTapeV2 { tape, session: self.session(), relations, max_tile_lanes })
    }
}
impl WideClaimsTapeV2<'_> {
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
            self.tape.read(at, &mut b[..n], self.session)?;
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
        self.tape.read(at, &mut n, self.session)?;
        at += 4;
        if u32::from_le_bytes(n) != self.relations {
            return Err("wide claims count changed".into());
        }
        for i in 0..self.relations {
            self.tape.read(at, &mut n, self.session)?;
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
                self.tape.read(at, &mut chunk[..bytes], self.session)?;
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
