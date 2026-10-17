//! **G14 R4: the scheduler of public convictions against ORIGINAL TIR frames, node side** (node policy; a child module of
//! `palw_fraud_filer`, dormant below `palw_tir_public_conviction_v1` — there the journal is emptied and nothing is queued).
//!
//! A finding — an `Arithmetic` cone/logits primitive, a complete EOF-authenticated `Output` ID sequence, or a greedy `DecodeToken`
//! pin, each over the frame's AUTHENTICATED original material — enters through [`PalwTirPublicFilerBookV1::queue_finding_v1`].
//! From there this module decides:
//!
//! - **when to file** — at once, inside the frame's fixed d0 door `[accepted, collect_until)`: the door never extends and the claim
//!   cannot reach `Final` before it closes, so filing early loses nothing and filing late can lose everything. Before it signs,
//!   the scheduler asks the fold's own path whether it would accept the filing at the next DAA
//!   (`palw_tir_public_conviction_preflight_v1`, a discarded dry run): a holding primitive, a liable or inactive filer, a claim
//!   already convicted or a liable bond without its exit hold is dropped here, so the node never pays a conviction rent the fold
//!   would refuse for a reason the accepted state already shows.
//! - **its queue lane** — the court queue's round [`PALW_TIR_PUBLIC_CONVICTION_ROUND_V1`] (`u32::MAX − 26`), one filing per claim,
//!   due by the door's last DAA (`collect_until − 1`) so the EDF carrier orders it against every other deadline. The carrier adds the
//!   frame's declared conviction rent on top of relay (`palw_lifecycle_carrier_rent_v2`). A lost carrier is re-queued after
//!   `COURT_MOVE_REPLAN_DAA`, at most [`PALW_TIR_PUBLIC_FILER_SENDS_V1`] times.
//! - **its restart journal** — every pending finding is persisted (`state_dir/palw-tir-public-filer.v1`, a temporary file renamed
//!   over the old one; an unreadable journal is set aside, never trusted), so a restart still files inside the door. An entry
//!   leaves when the fold shows the conviction (the preflight refuses it as already convicted), when the door closed, when the
//!   frame left the accepted state, or after its sends.
//!
//! **Not covered here (recorded in docs/design/palw/integration/g14-objective-conviction-remaining-2026-10-15.md):** the producers of
//! findings — an outside owner's acquisition driving (Open → Demand → Collect) and the arithmetic scheduler's localization to a
//! Ready primitive (`palw_tir_arithmetic_scheduler`) are not yet called from the panel loop.
#![allow(dead_code)] // `queue_finding_v1` is the hook the finding producers call once they are wired; the tick runs every panel tick.

use super::super::{COURT_MOVE_REPLAN_DAA, PalwPanelService};
use super::tir_public_filer::{
    palw_tir_public_conviction_object_v1, palw_tir_public_may_file_v1, palw_tir_public_witness_part_object_v1,
};
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_consensus_core::{
    Hash64,
    palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2},
    palw_tir_public_carriage_v1::PalwTirPublicConvictionEvidenceV1,
};
use kaspa_core::{debug, info, warn};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

const PALW_PANEL: &str = "palw-panel";

/// The court queue's lane of a public conviction against an ORIGINAL frame (free: `u32::MAX − 26`; the custodian's lanes are
/// `− 24` and `− 25`). Keyed by claim with `responder = false`: this node accuses.
pub(crate) const PALW_TIR_PUBLIC_CONVICTION_ROUND_V1: u32 = u32::MAX - 26;
/// Sends of one filing before it is given up (each a full conviction rent if it lands; a landed one leaves by the preflight).
pub(crate) const PALW_TIR_PUBLIC_FILER_SENDS_V1: u8 = 3;
/// Pending findings held at once (each at most one bounded control, 90,000 B).
pub(crate) const PALW_TIR_PUBLIC_FILER_PENDING_V1: usize = 16;
const PALW_TIR_PUBLIC_FILER_MAGIC_V1: &[u8] = b"misaka-node/tir-public-filer/v1\n";

/// One finding waiting to be filed: the evidence over authenticated public material, when it was found, how often it was sent.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub(crate) struct PalwTirPublicPendingFilingV1 {
    pub claim: Hash64,
    pub evidence: PalwTirPublicConvictionEvidenceV1,
    pub found_daa: u64,
    pub sends: u8,
    /// G14 R7: for a witness carried in parts, the offset of the part last sent (sends restart when the fold accepts it).
    pub staged_offset: u64,
}

/// **The filer's book**: the pending findings by claim, persisted.
#[derive(Default)]
pub(crate) struct PalwTirPublicFilerBookV1 {
    path: Option<PathBuf>,
    filings: BTreeMap<Hash64, PalwTirPublicPendingFilingV1>,
    dirty: bool,
}

impl PalwTirPublicFilerBookV1 {
    /// The journal in `state_dir`, or an empty one — an unreadable file is set aside (`.unreadable`), never a reason not to start.
    pub(crate) fn load(state_dir: &Path) -> Self {
        let path = state_dir.join("palw-tir-public-filer.v1");
        let mut book = Self { path: Some(path.clone()), ..Self::default() };
        let Ok(meta) = std::fs::metadata(&path) else { return book };
        let maximum = (PALW_TIR_PUBLIC_FILER_PENDING_V1 as u64).saturating_mul(
            kaspa_consensus_core::palw_tir_public_carriage_v1::PALW_TIR_PUBLIC_CONTROL_MAX_BYTES_V1.saturating_add(1024),
        );
        if meta.len() > maximum {
            warn!("[{PALW_PANEL}] the TIR public filer journal {} exceeds its bound; set aside", path.display());
            let _ = std::fs::rename(&path, path.with_extension("v1.unreadable"));
            return book;
        }
        let Ok(bytes) = std::fs::read(&path) else { return book };
        let parsed = bytes
            .strip_prefix(PALW_TIR_PUBLIC_FILER_MAGIC_V1)
            .and_then(|body| borsh::from_slice::<Vec<PalwTirPublicPendingFilingV1>>(body).ok())
            .filter(|entries| {
                entries.len() <= PALW_TIR_PUBLIC_FILER_PENDING_V1 && entries.iter().all(|e| e.evidence.check_staged_v1().is_ok())
            });
        match parsed {
            Some(entries) => {
                book.filings = entries.into_iter().map(|entry| (entry.claim, entry)).collect();
                if !book.filings.is_empty() {
                    info!(
                        "[{PALW_PANEL}] the TIR public filer resumes {} pending filing(s) from {}",
                        book.filings.len(),
                        path.display()
                    );
                }
            }
            None => {
                warn!("[{PALW_PANEL}] the TIR public filer journal {} does not decode; set aside", path.display());
                let _ = std::fs::rename(&path, path.with_extension("v1.unreadable"));
            }
        }
        book
    }

    /// Write the journal if it changed (temporary file, then rename).
    pub(crate) fn persist(&mut self) {
        if !self.dirty {
            return;
        }
        let Some(path) = self.path.clone() else {
            self.dirty = false;
            return;
        };
        let mut bytes = PALW_TIR_PUBLIC_FILER_MAGIC_V1.to_vec();
        let entries: Vec<&PalwTirPublicPendingFilingV1> = self.filings.values().collect();
        match borsh::to_vec(&entries) {
            Ok(body) => bytes.extend(body),
            Err(e) => {
                warn!("[{PALW_PANEL}] cannot encode the TIR public filer journal: {e}");
                return;
            }
        }
        let temporary = path.with_extension("v1.tmp");
        let written = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(&temporary, &bytes));
        match written.and_then(|()| std::fs::rename(&temporary, &path)) {
            Ok(()) => self.dirty = false,
            Err(e) => warn!(
                "[{PALW_PANEL}] cannot persist the TIR public filer journal {}: {e} — a restart would lose a filing",
                path.display()
            ),
        }
    }

    /// **The hook every finding producer calls.** Shape-checked here (never adjudicated: the fold does that, and the scheduler's
    /// preflight asks it). One finding per claim — a claim is convicted once, so a second finding adds nothing — and at most
    /// [`PALW_TIR_PUBLIC_FILER_PENDING_V1`] at once. Persisted before it returns `true`.
    pub(crate) fn queue_finding_v1(&mut self, claim: Hash64, evidence: PalwTirPublicConvictionEvidenceV1, now_daa: u64) -> bool {
        if evidence.check_staged_v1().is_err()
            || self.filings.contains_key(&claim)
            || self.filings.len() >= PALW_TIR_PUBLIC_FILER_PENDING_V1
        {
            return false;
        }
        self.filings.insert(claim, PalwTirPublicPendingFilingV1 { claim, evidence, found_daa: now_daa, sends: 0, staged_offset: 0 });
        self.dirty = true;
        self.persist();
        true
    }

    fn drop_v1(&mut self, claim: &Hash64, why: &str) {
        if self.filings.remove(claim).is_some() {
            self.dirty = true;
            debug!("[{PALW_PANEL}] TIR frame {claim}: public filing dropped: {why}");
        }
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.filings.len()
    }
}

impl PalwPanelService {
    /// **One tick of the TIR public filer scheduler** (see the module doc). `filer` is this node's bond: a seat or an outsider
    /// alike (the fold refuses a liable one). Never fails the loop.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn tir_public_filer_tick_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        filer: PalwBondKeyV2,
        network_domain: Hash64,
        current_daa: u64,
        book: &mut PalwTirPublicFilerBookV1,
        pending: &mut Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
        due: &mut HashMap<(Hash64, u32, bool), u64>,
        moved: &mut HashMap<(Hash64, u32, bool), u64>,
    ) {
        let ours = |round: u32, responder: bool| round == PALW_TIR_PUBLIC_CONVICTION_ROUND_V1 && !responder;
        if self.consensus_config.params.palw_tir_public_conviction_v1_fence().is_none() {
            if !book.filings.is_empty() {
                book.filings.clear();
                book.dirty = true;
                book.persist();
            }
            pending.retain(|(_, round, responder, _)| !ours(*round, *responder));
            return;
        }
        // A queued filing whose finding left the book is stale.
        pending.retain(|(claim, round, responder, _)| !ours(*round, *responder) || book.filings.contains_key(claim));
        let claims: Vec<Hash64> = book.filings.keys().copied().collect();
        for claim in claims {
            let key = (claim, PALW_TIR_PUBLIC_CONVICTION_ROUND_V1, false);
            if pending.iter().any(|(a, b, c, _)| (*a, *b, *c) == key)
                || moved.get(&key).is_some_and(|at| current_daa < at.saturating_add(COURT_MOVE_REPLAN_DAA))
            {
                continue;
            }
            let view = match session.palw_tir_public_frame_v1(claim) {
                Ok(Some(view)) => view,
                Ok(None) => {
                    book.drop_v1(&claim, "the frame is not (or no longer) in the accepted state");
                    continue;
                }
                Err(why) => {
                    debug!("[{PALW_PANEL}] TIR frame {claim}: frame reader unavailable: {why}");
                    continue;
                }
            };
            if current_daa >= view.collect_until_daa {
                book.drop_v1(&claim, "the frame's conviction door closed");
                continue;
            }
            if let Err(why) = palw_tir_public_may_file_v1(&view, &filer, current_daa) {
                // Before the door opens nothing is lost by waiting; a liable filer never may.
                if view.liable.contains(&filer) {
                    book.drop_v1(&claim, &why);
                }
                continue;
            }
            let Some(entry) = book.filings.get(&claim).cloned() else { continue };
            match session.palw_tir_public_conviction_preflight_v1(filer, claim, entry.evidence.clone()) {
                Ok(Some(Ok(()))) => {}
                Ok(Some(Err(why))) => {
                    // Already convicted (by this node's earlier send or anyone's), a holding primitive, or a refusal the accepted
                    // state already shows: filing would only burn the rent.
                    book.drop_v1(&claim, &why);
                    continue;
                }
                Ok(None) => continue,
                Err(why) => {
                    debug!("[{PALW_PANEL}] TIR frame {claim}: conviction preflight unavailable: {why}");
                    continue;
                }
            }
            // G14 R7: an evidence whose encoding does not fit one control rides in 16 KiB parts (split carriage, never truncated);
            // the fold adjudicates when the last part lands. The next part starts where the fold's row says it stopped.
            let encoded = match borsh::to_vec(&entry.evidence) {
                Ok(encoded) => encoded,
                Err(why) => {
                    book.drop_v1(&claim, &why.to_string());
                    continue;
                }
            };
            // Staged too: an Output past the single-control ID preflight, however few bytes it is.
            let staged = entry.evidence.check_v1().is_err()
                || encoded.len() as u64 + 4096
                    > kaspa_consensus_core::palw_tir_public_carriage_v1::PALW_TIR_PUBLIC_CONTROL_MAX_BYTES_V1;
            let built = if staged {
                let digest = kaspa_consensus_core::palw_tir_public_carriage_v1::palw_tir_public_witness_digest_v1(&encoded);
                let mut accepted: Option<(u64, Hash64)> = None;
                let mut read = |state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2| -> Result<(), String> {
                    use kaspa_consensus_core::palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key, PalwStagedCourtRowV1 as Row};
                    accepted = match state.staged_court_row_v1(&Key::TirWitness(claim, filer)) {
                        Some(Row::TirWitness(row)) => Some((row.received.len() as u64, row.digest)),
                        _ => None,
                    };
                    Ok(())
                };
                if !matches!(session.palw_tir_public_read_v1(&mut read), Ok(true)) {
                    continue;
                }
                let offset = match accepted {
                    None => 0,
                    Some((offset, row_digest)) if row_digest == digest => offset,
                    Some(_) => {
                        book.drop_v1(&claim, "this filer already stages another witness on the claim");
                        continue;
                    }
                };
                if offset != entry.staged_offset
                    && let Some(e) = book.filings.get_mut(&claim)
                {
                    e.staged_offset = offset;
                    e.sends = 0;
                    book.dirty = true;
                }
                palw_tir_public_witness_part_object_v1(
                    |message, context| self.sign(message, context).unwrap_or_default(),
                    network_domain,
                    &view,
                    filer,
                    current_daa,
                    &encoded,
                    offset,
                )
            } else {
                palw_tir_public_conviction_object_v1(
                    |message, context| self.sign(message, context).unwrap_or_default(),
                    network_domain,
                    &view,
                    filer,
                    current_daa,
                    entry.evidence,
                )
            };
            let object = match built {
                Ok(object) => object,
                Err(why) => {
                    book.drop_v1(&claim, &why);
                    continue;
                }
            };
            // Sends are bounded per carried part: a part the fold accepted restarts the count (above).
            if book.filings.get(&claim).is_some_and(|e| e.sends >= PALW_TIR_PUBLIC_FILER_SENDS_V1) {
                book.drop_v1(&claim, "sent its bounded number of times");
                continue;
            }
            pending.push((key.0, key.1, key.2, object));
            due.insert(key, view.collect_until_daa.saturating_sub(1));
            moved.insert(key, current_daa);
            if let Some(entry) = book.filings.get_mut(&claim) {
                entry.sends = entry.sends.saturating_add(1);
                book.dirty = true;
            }
            info!("[{PALW_PANEL}] TIR frame {claim}: public conviction queued (door closes at DAA {})", view.collect_until_daa);
        }
        book.persist();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }
    fn output(n: u32) -> PalwTirPublicConvictionEvidenceV1 {
        PalwTirPublicConvictionEvidenceV1::Output { rows_root: h(20), ids: vec![n, n + 1] }
    }

    /// One finding per claim, bounded, shape-checked; the journal survives a restart and a corrupt one is set aside.
    #[test]
    fn findings_are_bounded_once_per_claim_and_survive_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let mut book = PalwTirPublicFilerBookV1::load(dir.path());
        assert!(book.queue_finding_v1(h(1), output(1), 100));
        assert!(!book.queue_finding_v1(h(1), output(2), 101), "a claim is convicted once; its second finding adds nothing");
        assert!(
            !book.queue_finding_v1(
                h(2),
                PalwTirPublicConvictionEvidenceV1::Output { rows_root: Hash64::default(), ids: vec![1] },
                101
            ),
            "a shape-invalid finding is never queued"
        );
        for n in 2..(PALW_TIR_PUBLIC_FILER_PENDING_V1 as u64 + 5) {
            book.queue_finding_v1(h(n), output(n as u32), 102);
        }
        assert_eq!(book.len(), PALW_TIR_PUBLIC_FILER_PENDING_V1);
        let reloaded = PalwTirPublicFilerBookV1::load(dir.path());
        assert_eq!(reloaded.filings, book.filings);
        assert_eq!(reloaded.filings[&h(1)].evidence, output(1));
        std::fs::write(dir.path().join("palw-tir-public-filer.v1"), b"garbage").unwrap();
        assert_eq!(PalwTirPublicFilerBookV1::load(dir.path()).len(), 0);
        assert!(dir.path().join("palw-tir-public-filer.v1.unreadable").exists());
    }
}
