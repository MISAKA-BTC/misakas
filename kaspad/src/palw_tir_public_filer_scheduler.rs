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
//!   over the old one; stable malformed syntax alone is set aside). Local IO/capacity keeps a loading/write barrier and
//!   retries from the actual owner/filer tick; reloaded bytes restore opaque evidence, never current authority. An entry
//!   leaves when the fold shows the conviction (the preflight refuses it as already convicted), when the door closed, when the
//!   frame left the accepted state, or after its sends.
//!
//! **Remaining:** full allocation/cost coverage of existing preflight/signing copies and OS/path/IO internals, plus the
//! funded interaction graph and live campaigns. The outside owner calls this actual queue; a reloaded finding must still pass
//! current native frame/clock/signing/preflight checks before a carrier is built.
#![allow(dead_code)] // `queue_finding_v1` is the hook the finding producers call once they are wired; the tick runs every panel tick.
use crate::palw_panel::court_outbox::CourtOutboxV1;

use super::super::{COURT_MOVE_REPLAN_DAA, PalwPanelService};
use super::tir_public_filer::{
    palw_tir_public_conviction_object_v1, palw_tir_public_may_file_v1, palw_tir_public_witness_part_object_v1,
};
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_consensus_core::{Hash64, palw_state_v2::PalwBondKeyV2, palw_tir_public_carriage_v1::PalwTirPublicConvictionEvidenceV1};
use kaspa_core::{debug, info, warn};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const PALW_PANEL: &str = "palw-panel";

#[path = "palw_tir_filer_journal_v2.rs"]
mod journal;
use journal::{JournalFaultV2,ReloadPayloadV2};

#[path = "palw_tir_filer_preparation_v2.rs"]
mod preparation;
use preparation::{FilerClaimListV2,PreparedFilingV2,FilerPreparationFaultV2};

/// The court queue's lane of a public conviction against an ORIGINAL frame (free: `u32::MAX − 26`; the custodian's lanes are
/// `− 24` and `− 25`). Keyed by claim with `responder = false`: this node accuses.
pub(crate) const PALW_TIR_PUBLIC_CONVICTION_ROUND_V1: u32 = u32::MAX - 26;
/// Sends of one filing before it is given up (each a full conviction rent if it lands; a landed one leaves by the preflight).
pub(crate) const PALW_TIR_PUBLIC_FILER_SENDS_V1: u8 = 3;
/// Pending findings held at once. A finding may span multiple bounded controls;
/// its complete staged evidence is retained and journaled under local admission.
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
    // Nonwire resources for new owned findings. Legacy journal reload does not
    // recreate them or turn disk bytes into current authority.
    owned_filings: Vec<(Hash64, super::tir_public_owner::cone_evidence_owned::ConeEvidenceHoldV2)>,
    loaded_filings: Vec<(Hash64, std::sync::Arc<ReloadPayloadV2>)>,
    loading: bool,
    // Keep the actual read and its completed result across cancellation of a tick.
    load_task: Option<tokio::task::JoinHandle<Result<Option<journal::JournalDecodedV2>, JournalFaultV2>>>,
    // Remains after clear/removal: Vec retains capacity. Field order ensures
    // both the pending values and sidecar buffer drop before this whole-table ticket.
    owned_membership: Option<(Option<kaspa_consensus_core::palw_native_resource_v1::NativeDemandLeaseV1>, crate::palw_memory_ledger::PalwMemoryReservationV1)>,
    dirty: bool,
}

impl PalwTirPublicFilerBookV1 {
    /// Read under byte/expanded-type admission. Local IO/capacity holds loading;
    /// only stable malformed syntax is set aside, never an empty-file overwrite.
    pub(crate) fn load(state_dir: &Path) -> Self {
        let path = state_dir.join("palw-tir-public-filer.v1");
        let mut book = Self { path: Some(path.clone()), loading: true, ..Self::default() };
        if let Err(why) = book.finish_load_v2(journal::load_attempt_v2(&path)) {
            debug!("[{PALW_PANEL}] TIR journal load remains pending: {why}");
        }
        book
    }
    fn finish_load_v2(&mut self, attempt: Result<Option<journal::JournalDecodedV2>, JournalFaultV2>) -> Result<(), JournalFaultV2> {
        let decoded = attempt?;
        let Some(decoded) = decoded else {
            self.loading = false;
            self.dirty = false;
            return Ok(());
        };
        if !self.filings.is_empty() {
            return Err(JournalFaultV2::Io("pending journal cannot replace live findings".into()));
        }
        if decoded.entries.is_empty() {
            self.loading = false;
            self.dirty = false;
            return Ok(());
        }
        if cfg!(not(target_pointer_width = "64")) {
            return Err(JournalFaultV2::Resource("pinned journal BTree profile requires64-bit target".into()));
        }
        use kaspa_consensus_core::{
            palw_native_resource_v1::{btree_node_envelope_bytes_v1, reserve_native_requests_v1},
            palw_routed_material_v2::PalwMaterialConstructionRequestsV2,
        };
        type Slot = (Hash64, std::sync::Arc<ReloadPayloadV2>);
        let count = decoded.entries.len();
        let mut q = PalwMaterialConstructionRequestsV2::new();
        let slots = self
            .loaded_filings
            .capacity()
            .checked_add(count)
            .ok_or_else(|| JournalFaultV2::Resource("journal slot overflow".into()))?;
        q.vec_v1::<Slot>(slots as u64).map_err(|e| JournalFaultV2::Resource(e.to_string()))?;
        q.vec_v1::<(Hash64, super::tir_public_owner::cone_evidence_owned::ConeEvidenceHoldV2)>(self.owned_filings.capacity() as u64)
            .map_err(|e| JournalFaultV2::Resource(e.to_string()))?;
        q.vec_v1::<ReloadPayloadV2>(1).map_err(|e| JournalFaultV2::Resource(e.to_string()))?;
        q.vec_v1::<usize>(3).map_err(|e| JournalFaultV2::Resource(e.to_string()))?;
        let nodes = count
            .checked_add(1)
            .and_then(|n| n.checked_mul(btree_node_envelope_bytes_v1::<Hash64, PalwTirPublicPendingFilingV1>()))
            .ok_or_else(|| JournalFaultV2::Resource("journal BTree membership overflow".into()))?;
        q.add_v1(nodes as u64).map_err(|e| JournalFaultV2::Resource(e.to_string()))?;
        let bytes = q.bytes_v1();
        let native = reserve_native_requests_v1(bytes).map_err(|e| JournalFaultV2::Resource(e.to_string()))?;
        let host = crate::palw_memory_ledger::host_ledger_v1()
            .reserve(
                crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
                    role: "public-tir-journal-membership",
                    class_id: Default::default(),
                    job: Default::default(),
                },
                bytes,
            )
            .map_err(|e| JournalFaultV2::Resource(e.to_string()))?;
        let mut holders = Vec::new();
        holders.try_reserve_exact(count).map_err(|e| JournalFaultV2::Resource(e.to_string()))?;
        let (entries, payload) = decoded.into_parts_v2();
        let payload = std::sync::Arc::new(payload);
        let mut filings = BTreeMap::new();
        for entry in entries {
            holders.push((entry.claim, std::sync::Arc::clone(&payload)));
            filings.insert(entry.claim, entry);
        }
        self.filings = filings;
        self.loaded_filings = holders;
        self.owned_membership = Some((native, host));
        self.loading = false;
        self.dirty = false;
        Ok(())
    }
    fn start_load_worker_v2(
        &mut self,
        loader: impl FnOnce(PathBuf) -> Result<Option<journal::JournalDecodedV2>, JournalFaultV2> + Send + 'static,
    ) {
        if self.load_task.is_none() {
            if let Some(path) = self.path.clone() {
                self.load_task = Some(tokio::task::spawn_blocking(move || loader(path)));
            }
        }
    }
    /// The book owns the handle as well as the worker's result. Cancelling a
    /// tick cannot detach it and start a second read on the next tick.
    pub(crate) async fn advance_journal_v2(&mut self) -> Result<(), JournalFaultV2> {
        if self.loading {
            self.start_load_worker_v2(|path| journal::load_attempt_v2(&path));
            let Some(task) = self.load_task.as_mut() else {
                self.loading = false;
                return Ok(());
            };
            let attempt = task.await;
            // Only a terminal worker is removed; an interrupted await keeps it.
            self.load_task.take();
            let attempt = attempt.map_err(|e| JournalFaultV2::Io(format!("journal read worker failed: {e}")))?;
            self.finish_load_v2(attempt)?;
        }
        self.try_persist_v2()
    }
    fn try_persist_v2(&mut self) -> Result<(), JournalFaultV2> {
        if self.loading {
            return Err(JournalFaultV2::Io("journal loading is pending; writes are held".into()));
        }
        if !self.dirty {
            return Ok(());
        }
        let Some(path) = self.path.as_ref() else {
            self.dirty = false;
            return Ok(());
        };
        let bytes = journal::encode_reserved_v2(&self.filings)?;
        journal::write_atomic_v2(path, &bytes)?;
        self.dirty = false;
        Ok(())
    }
    /// Local refusal/IO keeps dirty for the next real owner/filer tick. Never
    /// overwrite a journal whose initial read has not completed.
    pub(crate) fn persist(&mut self) {
        if self.loading {
            return;
        }
        if let Err(why) = self.try_persist_v2() {
            debug!("[{PALW_PANEL}] TIR journal persist remains pending: {why}");
        }
    }

    /// **The hook every finding producer calls.** Shape-checked here (never adjudicated: the fold does that, and the scheduler's
    /// preflight asks it). One finding per claim — a claim is convicted once, so a second finding adds nothing — and at most
    /// [`PALW_TIR_PUBLIC_FILER_PENDING_V1`] at once. Persisted before it returns `true`.
    pub(crate) fn queue_finding_v1(&mut self, claim: Hash64, evidence: PalwTirPublicConvictionEvidenceV1, now_daa: u64) -> bool {
        if self.loading {return false;}
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

    pub(crate) fn try_queue_owned_finding_v2(
        &mut self,
        claim: Hash64,
        evidence: super::tir_public_owner::cone_evidence_owned::OwnedConeEvidenceV2,
        now_daa: u64,
    ) -> Result<bool, kaspa_consensus_core::palw_backend::PalwTirPublicConeFaultV2> {
        use kaspa_consensus_core::{
            palw_backend::PalwTirPublicConeFaultV2 as Fault, palw_native_resource_v1::reserve_native_requests_v1,
            palw_routed_material_v2::PalwMaterialConstructionRequestsV2,
        };
        if self.loading{return Err(Fault::Unavailable("journal loading is pending".into()));}
        if cfg!(not(target_pointer_width = "64")) {
            return Err(Fault::LocalResource(kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1::unsupported::<Self>(
                "owned TIR pending membership", "pinned BTree allocation layout only covers 64-bit targets").to_string()));
        }
        if self.filings.contains_key(&claim) {
            return Ok(false);
        }
        if self.filings.len() >= PALW_TIR_PUBLIC_FILER_PENDING_V1 {
            return Err(Fault::LocalResource("local pending TIR finding queue is full".into()));
        }
        type Slot = (Hash64, super::tir_public_owner::cone_evidence_owned::ConeEvidenceHoldV2);
        let mut q = PalwMaterialConstructionRequestsV2::new();
        let slots = self
            .owned_filings
            .capacity()
            .checked_add(self.owned_filings.len() + 1)
            .ok_or_else(|| Fault::LocalResource("TIR finding membership overflow".into()))?;
        q.vec_v1::<Slot>(slots as u64).map_err(|e| Fault::LocalResource(e.to_string()))?;
        q.vec_v1::<(Hash64,std::sync::Arc<ReloadPayloadV2>)>(self.loaded_filings.capacity() as u64).map_err(|e|Fault::LocalResource(e.to_string()))?;
        if !self.loaded_filings.is_empty(){
            q.vec_v1::<ReloadPayloadV2>(1).map_err(|e|Fault::LocalResource(e.to_string()))?;
            q.vec_v1::<usize>(3).map_err(|e|Fault::LocalResource(e.to_string()))?;
        }
        // Pinned typed BTree envelope: old table plus at most one full node per
        // new entry, including split/root growth. No codec-byte multiplier.
        let nodes = self
            .filings
            .len()
            .checked_mul(2)
            .and_then(|n| n.checked_add(1))
            .ok_or_else(|| Fault::LocalResource("TIR pending tree overflow".into()))?;
        let tree = nodes
            .checked_mul(kaspa_consensus_core::palw_native_resource_v1::btree_node_envelope_bytes_v1::<
                Hash64,
                PalwTirPublicPendingFilingV1,
            >())
            .ok_or_else(|| Fault::LocalResource("TIR pending node bytes overflow".into()))?;
        q.add_v1(tree as u64).map_err(|e| Fault::LocalResource(e.to_string()))?;
        let bytes = q.bytes_v1();
        let native = reserve_native_requests_v1(bytes).map_err(|e| Fault::LocalResource(e.to_string()))?;
        let host = crate::palw_memory_ledger::host_ledger_v1()
            .reserve(
                crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
                    role: "public-tir-owned-finding",
                    class_id: Hash64::default(),
                    job: claim,
                },
                bytes,
            )
            .map_err(|e| Fault::LocalResource(e.to_string()))?;
        self.owned_filings.try_reserve_exact(1).map_err(|e| Fault::LocalResource(e.to_string()))?;
        let (evidence, hold) = evidence.into_parts_v2();
        self.filings.insert(claim, PalwTirPublicPendingFilingV1 { claim, evidence, found_daa: now_daa, sends: 0, staged_offset: 0 });
        self.owned_filings.push((claim, hold));
        // The new full-table lease already covers old/new allocations. Replace
        // the previous ticket only after all infallible insertions complete.
        self.owned_membership=Some((native,host));
        self.dirty = true;
        self.persist();
        Ok(true)
    }

    fn enqueue_filing_v2(
        &mut self,
        claim: Hash64,
        object: &kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2,
        current_daa: u64,
        due: u64,
        outbox: &mut CourtOutboxV1,
    ) -> Result<(), kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1> {
        let key = (claim, PALW_TIR_PUBLIC_CONVICTION_ROUND_V1, false);
        outbox.try_push_filer_clone_with_due_v2(key, object, due)?;
        outbox.mark_submitted(key, current_daa);
        if let Some(entry) = self.filings.get_mut(&claim) {
            entry.sends = entry.sends.saturating_add(1);
            self.dirty = true;
        }
        Ok(())
    }

    pub(super) fn drop_v1(&mut self, claim: &Hash64, why: &str) {
        if self.filings.remove(claim).is_some() {
            self.owned_filings.retain(|(key,_)|key!=claim);
            self.loaded_filings.retain(|(key,_)|key!=claim);
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
        court_outbox: &mut CourtOutboxV1,
    ) {
        let ours = |round: u32, responder: bool| round == PALW_TIR_PUBLIC_CONVICTION_ROUND_V1 && !responder;
        if self.consensus_config.params.palw_tir_public_conviction_v1_fence().is_none() {
            if !book.filings.is_empty() {
                book.filings.clear();
                book.owned_filings.clear();
                book.loaded_filings.clear();
                book.dirty = true;
                book.persist();
            }
            court_outbox.retain_pending(|(_, round, responder, _)| !ours(*round, *responder));
            return;
        }
        if let Err(why) = book.advance_journal_v2().await {
            debug!("[{PALW_PANEL}] TIR filer waits for its journal: {why}");
            return;
        }
        // A queued filing whose finding left the book is stale.
        court_outbox.retain_pending(|(claim, round, responder, _)| !ours(*round, *responder) || book.filings.contains_key(claim));
        let claims = match FilerClaimListV2::new(book.filings.keys()) {
            Ok(claims) => claims,
            Err(why) => {
                debug!("[{PALW_PANEL}] TIR filer claim workspace unavailable: {why:?}");
                return;
            }
        };
        for &claim in &claims.claims {
            let key = (claim, PALW_TIR_PUBLIC_CONVICTION_ROUND_V1, false);
            if court_outbox.pending().iter().any(|(a, b, c, _)| (*a, *b, *c) == key)
                || court_outbox.submitted_at(&key).is_some_and(|at| current_daa < at.saturating_add(COURT_MOVE_REPLAN_DAA))
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
            let Some(entry) = book.filings.get(&claim) else { continue };
            let staged_offset = entry.staged_offset;
            let prepared = match PreparedFilingV2::new(&entry.evidence, &view, filer, network_domain) {
                Ok(prepared) => prepared,
                Err(FilerPreparationFaultV2::Resource(why)) => {
                    debug!("[{PALW_PANEL}] TIR frame {claim}: preparation deferred: {why}");
                    continue;
                }
                Err(FilerPreparationFaultV2::Semantic(why)) => {
                    book.drop_v1(&claim, &why);
                    continue;
                }
            };
            let preflight = if prepared.staged {
                session.palw_tir_public_staged_conviction_preflight_v2(filer, claim, &prepared.evidence)
            } else {
                session.palw_tir_public_conviction_preflight_v1(filer, claim, prepared.evidence.clone())
            };
            match preflight {
                Ok(Some(Ok(()))) => {}
                Ok(Some(Err(why))) => {
                    book.drop_v1(&claim, &why);
                    continue;
                }
                Ok(None) => continue,
                Err(why) => {
                    debug!("[{PALW_PANEL}] TIR frame {claim}: conviction preflight unavailable: {why}");
                    continue;
                }
            }
            let encoded = &prepared.encoded;
            let built = if prepared.staged {
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
                if offset != staged_offset
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
                    prepared.evidence.clone(),
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
            if let Err(why) =
                book.enqueue_filing_v2(claim, &object, current_daa, view.collect_until_daa.saturating_sub(1), court_outbox)
            {
                debug!("[{PALW_PANEL}] TIR frame {claim}: destination admission deferred: {why}");
                continue;
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

#[cfg(test)]
#[path = "palw_tir_filer_preparation_tests_v2.rs"]
mod preparation_tests;
