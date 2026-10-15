//! **G14 R3/R8: the custodian preparation of an ORIGINAL TIR frame, node side** (node policy; a child module of
//! `palw_fraud_filer`, dormant below `palw_tir_public_conviction_v1` — there the accepted-state reader answers `Ok(false)`, the
//! book's live cases are dropped and nothing is queued, so a node below the fence behaves byte for byte as before).
//!
//! Two literal preparers use the ONE claim-wide preparation lane the producer prepaid at d0 (`Key::TirPreparation`):
//!
//! - **the producer** — a custodian from d0. Its pending duty is read off the accepted index
//!   (`staged_tir_preparation_duties_after_v1`); once its capture is prepared it files its own `PreparationReady`.
//! - **a drawn seat that signed `Valid` (late grant)** — it prepares AHEAD on the same paid lane while it is still only drawn
//!   (`capture_debit_v1` admits the producer and the drawn panel), and grants raw custody only once its capture is sealed, in
//!   ONE atomic object (`TirPublicGrantReadyV1`: its ordinary Valid credit + the raw custody grant + Ready). The grant therefore
//!   never carries a cold-preparation deadline it could miss: no liability exists until the prepared capture does.
//!
//! **One slice, one paid permit.** For each slice the backend quotes the work (`slice_quote_v1`: the cold reader's phases, then
//! the prepared writer's publication phase); the consensus work book prepares the EXACT debit on the accepted state; the signed
//! `WorkDebit` control rides the court queue (its own lane, [`PALW_TIR_PUBLIC_DEBIT_ROUND_V1`]); and only once the debit is
//! ACCEPTED does `confirm_v1` mint the permit that funds the slice (`tir_acquisition_driver`'s `prepare_claim_*_slice_v1`). A lost
//! carrier re-sends the same exact debit (`pending_action_v1`); a slice is never run on an unaccepted debit.
//!
//! **Restart.** The sealed descriptor of every prepared capture is journaled (`state_dir/palw-tir-public-custodian.v1`, written
//! to a temporary file then renamed), so a restart re-files a lost Ready without preparing again and the responder can reopen the
//! exact sealed capture (`reopen_tir_prepared_capture_v1`). An unsealed preparation restarts from its first slice on a fresh
//! reader; the work book marks the first debit after a restart (`restart: true`) and the native per-actor slice budget bounds it.
//!
//! **Not covered here (recorded in docs/design/palw/g14-objective-conviction-remaining-2026-10-15.md):** answering outside owners'
//! demands from the prepared capture (the Begin/Part/Finish responder), measured completion within the fixed preparation clock at
//! the largest profile, and GC of sealed captures past the material horizon.
#![allow(dead_code)] // the panel loop calls the tick; the journal helpers are exercised by the tests below.

use super::super::{COURT_MOVE_REPLAN_DAA, PalwPanelService};
use super::tir_acquisition_driver::{
    BorrowedPreparationSnapshotV1, OperationalLedgerV1, preparation_ready_action_v1, preparation_slice_work_v1,
    prepare_claim_capture_slice_v1, prepare_claim_publication_slice_v1,
};
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_consensus_core::{
    Hash64,
    palw_backend::{
        PalwOriginalCaptureReaderV1, PalwTirPublicCaptureReaderV1, PalwTirPublicCaptureSliceV1, PalwTirPublicPreparedDescriptorV1,
        PalwTirPublicPreparedWriterV1,
    },
    palw_panel_v2::{PalwReceiptVerdictV2, PalwSeatReceiptV2},
    palw_staged_court_state_v1::PalwStagedCourtKeyV1 as Key,
    palw_state_v2::{PalwBondKeyV2, PalwChainStateV2, PalwConsensusObjectV2},
    palw_tir_public_carriage_v1::{
        PALW_TIR_PUBLIC_CONTROL_CONTEXT_V1, PalwTirPublicControlActionV1, PalwTirPublicControlTargetV1, PalwTirPublicControlV1,
    },
    palw_tir_public_claim_v1::{PalwTirPublicFrameViewV1, palw_tir_public_claim_v1},
    palw_tir_public_owned_v1::{PalwTirPublicOwnedActionV1, PalwTirPublicWorkPhaseV1, PalwTirPublicWorkScopeV1, PalwTirPublicWorkV1},
    palw_tir_public_source_v1::PalwTirPublicRequestV1,
    palw_tir_public_state_v1::{
        PalwTirPublicPreparationSnapshotV1, PalwTirPublicWorkBookV1, PalwTirPublicWorkPermitV1,
        palw_tir_public_preparation_snapshot_v1, palw_tir_public_work_book_v1,
    },
};
use kaspa_core::{debug, info, warn};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const PALW_PANEL: &str = "palw-panel";

/// The court queue's lane of a preparer's `WorkDebit` on a frame (free: `u32::MAX − 24`; the TIR prefix responder is `− 23`).
pub(crate) const PALW_TIR_PUBLIC_DEBIT_ROUND_V1: u32 = u32::MAX - 24;
/// The court queue's lane of a preparer's Ready (the producer's `PreparationReady`, a seat's `TirPublicGrantReadyV1`).
pub(crate) const PALW_TIR_PUBLIC_READY_ROUND_V1: u32 = u32::MAX - 25;
/// Frames this node prepares at once (each holds one capture reader and, once warm, one prepared writer).
const PALW_TIR_PUBLIC_CASES_V1: usize = 4;
/// The journal's bound; a sealed entry leaves once native state shows its Ready, or its material horizon passed.
const PALW_TIR_PUBLIC_SEALED_MAX_V1: usize = 4096;
const PALW_TIR_PUBLIC_JOURNAL_MAGIC_V1: &[u8] = b"misaka-node/tir-public-custodian/v1\n";
/// The fixed per-slice overhead `OperationalLedgerV1::require_preparation_v1` debits before the slice's own quoted work, so
/// the debit the chain accepts covers both (it never under-buys and then refuses its own slice).
const PALW_TIR_PUBLIC_SLICE_OVERHEAD_V1: PalwTirPublicWorkV1 = PalwTirPublicWorkV1 {
    attempts: 1,
    io_bytes: 0,
    hash_bytes: 8192,
    copy_bytes: 0,
    parser_steps: 1024,
    id_copy_words: 0,
    tree_hashes: 0,
    replay_leaves: 0,
    replay_positions: 0,
    replay_id_words: 0,
    arithmetic_work: 0,
    traffic_bytes: 0,
    eval_elements: 0,
    eval_terms: 0,
    source_reads: 0,
    target_requests: 0,
};

/// One sealed prepared capture: what a restart needs to re-file Ready and to reopen the capture for answers.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub(crate) struct PalwTirPublicSealedV1 {
    pub claim: Hash64,
    pub allocation_id: Hash64,
    pub signer: PalwBondKeyV2,
    pub material_until_daa: u64,
    pub descriptor: PalwTirPublicPreparedDescriptorV1,
}

/// One frame this node is preparing (process memory; see the module's restart note).
struct CaseV1 {
    work: PalwTirPublicWorkBookV1,
    reader: Option<Box<dyn PalwTirPublicCaptureReaderV1>>,
    writer: Option<Box<dyn PalwTirPublicPreparedWriterV1>>,
    warm: bool,
    /// The quote whose debit is queued or in flight, and the permit once that debit is accepted.
    quote: Option<PalwTirPublicCaptureSliceV1>,
    permit: Option<PalwTirPublicWorkPermitV1>,
    failed: bool,
    _capture: Option<crate::palw_memory_ledger::PalwMemoryReservationV1>,
    _publish: Option<crate::palw_memory_ledger::PalwMemoryReservationV1>,
}

/// **The custodian book**: the live preparations (memory) and the sealed-descriptor journal (disk).
#[derive(Default)]
pub(crate) struct PalwTirPublicCustodianBookV1 {
    path: Option<PathBuf>,
    sealed: BTreeMap<(Hash64, Hash64), PalwTirPublicSealedV1>,
    dirty: bool,
    cases: HashMap<(Hash64, Hash64), CaseV1>,
}

impl PalwTirPublicCustodianBookV1 {
    /// The journal in `state_dir`, or an empty one — an unreadable file is set aside (`.unreadable`), never a reason not to start.
    pub(crate) fn load(state_dir: &Path) -> Self {
        let path = state_dir.join("palw-tir-public-custodian.v1");
        let mut book = Self { path: Some(path.clone()), ..Self::default() };
        let Ok(meta) = std::fs::metadata(&path) else { return book };
        if meta.len() > (PALW_TIR_PUBLIC_SEALED_MAX_V1 as u64).saturating_mul(1024) {
            warn!("[{PALW_PANEL}] the TIR custodian journal {} exceeds its bound; set aside", path.display());
            let _ = std::fs::rename(&path, path.with_extension("v1.unreadable"));
            return book;
        }
        let Ok(bytes) = std::fs::read(&path) else { return book };
        let parsed = bytes
            .strip_prefix(PALW_TIR_PUBLIC_JOURNAL_MAGIC_V1)
            .and_then(|body| borsh::from_slice::<Vec<PalwTirPublicSealedV1>>(body).ok())
            .filter(|entries| entries.len() <= PALW_TIR_PUBLIC_SEALED_MAX_V1);
        match parsed {
            Some(entries) => {
                book.sealed = entries.into_iter().map(|entry| ((entry.claim, entry.allocation_id), entry)).collect();
                if !book.sealed.is_empty() {
                    info!(
                        "[{PALW_PANEL}] the TIR custodian resumes {} sealed prepared capture(s) from {}",
                        book.sealed.len(),
                        path.display()
                    );
                }
            }
            None => {
                warn!("[{PALW_PANEL}] the TIR custodian journal {} does not decode; set aside", path.display());
                let _ = std::fs::rename(&path, path.with_extension("v1.unreadable"));
            }
        }
        book
    }

    /// Write the journal if it changed: a temporary file renamed over the old one, so a crash never leaves half a journal.
    pub(crate) fn persist(&mut self) {
        if !self.dirty {
            return;
        }
        let Some(path) = self.path.clone() else {
            self.dirty = false;
            return;
        };
        let mut bytes = PALW_TIR_PUBLIC_JOURNAL_MAGIC_V1.to_vec();
        let entries: Vec<&PalwTirPublicSealedV1> = self.sealed.values().collect();
        match borsh::to_vec(&entries) {
            Ok(body) => bytes.extend(body),
            Err(e) => {
                warn!("[{PALW_PANEL}] cannot encode the TIR custodian journal: {e}");
                return;
            }
        }
        let temporary = path.with_extension("v1.tmp");
        let written = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(&temporary, &bytes));
        match written.and_then(|()| std::fs::rename(&temporary, &path)) {
            Ok(()) => self.dirty = false,
            Err(e) => warn!("[{PALW_PANEL}] cannot persist the TIR custodian journal {}: {e}", path.display()),
        }
    }

    /// The sealed prepared capture of `(claim, allocation)`, for the responder's reopen.
    pub(crate) fn sealed_v1(&self, claim: Hash64, allocation_id: Hash64) -> Option<&PalwTirPublicSealedV1> {
        self.sealed.get(&(claim, allocation_id))
    }

    fn seal_v1(&mut self, entry: PalwTirPublicSealedV1) -> Result<(), String> {
        if self.sealed.len() >= PALW_TIR_PUBLIC_SEALED_MAX_V1 && !self.sealed.contains_key(&(entry.claim, entry.allocation_id)) {
            return Err("the TIR custodian journal is full; the sealed capture is kept in memory only".into());
        }
        self.sealed.insert((entry.claim, entry.allocation_id), entry);
        self.dirty = true;
        Ok(())
    }

    /// Drop sealed entries whose material horizon passed at `now` (their capture is no longer anyone's duty).
    fn expire_v1(&mut self, now: u64) {
        let before = self.sealed.len();
        self.sealed.retain(|_, entry| now < entry.material_until_daa);
        self.dirty |= before != self.sealed.len();
    }
}

/// What one accepted read decided for one frame; acted on after the read (backend IO and signing happen outside it).
enum StepV1 {
    /// Open the cold capture reader of the frame's original (backend + retained capture).
    Open {
        key: (Hash64, Hash64),
        header: Box<kaspa_consensus_core::palw_public_binding_v1::PalwPublicTirBindingHeaderV1>,
        job: Box<kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3>,
        producer: bool,
    },
    /// The reader is warm: begin the prepared writer at the frame's native material horizon.
    Begin { key: (Hash64, Hash64), material_until_daa: u64, now: u64 },
    /// Queue (or re-queue) this exact signed debit.
    Debit { key: (Hash64, Hash64), action: PalwTirPublicOwnedActionV1, original_accepted_daa: u64, source_id: Hash64, due: u64 },
    /// The debit was accepted and its permit minted: run the slice on this snapshot.
    Run { key: (Hash64, Hash64), snapshot: Box<PalwTirPublicPreparationSnapshotV1> },
    /// The capture is sealed: file Ready (the producer's own, or a seat's atomic grant + Ready).
    Ready {
        key: (Hash64, Hash64),
        snapshot: Box<PalwTirPublicPreparationSnapshotV1>,
        view: Option<Box<PalwTirPublicFrameViewV1>>,
        descriptor: PalwTirPublicPreparedDescriptorV1,
    },
}

/// The literal preparation duty `signer` has on `(claim, allocation)` at the accepted snapshot: `Some(is_producer)` while it is
/// pending (the producer as a d0 custodian; a drawn seat that signed Valid and holds no grant yet), `None` otherwise.
fn preparation_duty_v1(
    state: &PalwChainStateV2,
    now: u64,
    signer: PalwBondKeyV2,
    claim: Hash64,
    allocation_id: Hash64,
) -> Option<bool> {
    let row = state.staged_tir_preparation_v1(claim, allocation_id)?;
    let p = &row.preparation;
    let c = p.contract_v1();
    if now < c.original_accepted_daa || now >= c.preparation_until_daa || p.ready_v1(&signer).is_some() || p.defaulted_v1(&signer) {
        return None;
    }
    if signer == c.producer {
        return Some(true);
    }
    (c.drawn_panel.contains(&signer) && !c.custodians.contains(&signer)).then_some(false)
}

/// The retained original capture of `claim` as a byte-range reader (the producer's own capture, or the copy this seat retained
/// when it verified): an FPC1 envelope is scoped to the exact public job, a raw TIR capture is read as it is. Anything else —
/// a job-only or answer-only file — is not an original capture and is skipped.
fn retained_original_v1(
    paths: &[PathBuf],
    job: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
) -> Result<Arc<dyn PalwOriginalCaptureReaderV1>, String> {
    use misaka_palw_base0::flat_public_source::{Base0OriginalCaptureFileV1, Base0OriginalCaptureRangeV1};
    for path in paths {
        let Ok(file) = Base0OriginalCaptureFileV1::open_v1(path) else { continue };
        let file = Arc::new(file);
        let Ok(magic) = file.read_v1(0, file.len_v1().min(8) as u32) else { continue };
        if magic.starts_with(b"FPC1") {
            if let Ok(range) = Base0OriginalCaptureRangeV1::fp_capture_v1(file, job) {
                return Ok(Arc::new(range));
            }
        } else if magic == misaka_palw_sdk::lineages::tir::TirCaptureV1::MAGIC {
            return Ok(file);
        }
    }
    Err("this node retains no original TIR capture of the claim".into())
}

fn signed_control_v1(
    service: &PalwPanelService,
    network_domain: Hash64,
    mut control: PalwTirPublicControlV1,
) -> Result<PalwConsensusObjectV2, String> {
    let digest = control.signing_message_v1(network_domain)?;
    control.signature =
        service.sign(digest.as_byte_slice(), PALW_TIR_PUBLIC_CONTROL_CONTEXT_V1).ok_or("TIR custodian signing key unavailable")?;
    control.check_v1()?;
    Ok(PalwConsensusObjectV2::TirPublicControlV1 { control: Box::new(control) })
}

impl PalwPanelService {
    /// **One tick of the custodian preparation** (see the module doc). `own_receipts` are this seat's signed receipts (a seat
    /// prepares only a claim it signed `Valid`); the producer's duties come from the accepted index. Never fails the loop: a
    /// frame whose step errs is retried on a later tick (or dropped once its duty ended).
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn tir_public_custodian_tick_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        signer: PalwBondKeyV2,
        network_domain: Hash64,
        own_receipts: &HashMap<Hash64, (PalwSeatReceiptV2, u64)>,
        book: &mut PalwTirPublicCustodianBookV1,
        pending: &mut Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
        due: &mut HashMap<(Hash64, u32, bool), u64>,
        moved: &mut HashMap<(Hash64, u32, bool), u64>,
    ) -> Result<(), String> {
        let ours = |round: u32, responder: bool| {
            responder && (round == PALW_TIR_PUBLIC_DEBIT_ROUND_V1 || round == PALW_TIR_PUBLIC_READY_ROUND_V1)
        };
        if self.consensus_config.params.palw_tir_public_conviction_v1_fence().is_none() {
            book.cases.clear();
            pending.retain(|(_, round, responder, _)| !ours(*round, *responder));
            return Ok(());
        }
        // ---- ONE accepted read: discover duties, confirm accepted debits, prepare the next exact debits. ----
        let mut steps: Vec<StepV1> = Vec::new();
        let mut now = 0u64;
        let armed = {
            let mut read = |state: &PalwChainStateV2| -> Result<(), String> {
                steps.clear();
                now = state.last_point().ok_or("TIR custodian snapshot has no accepted clock")?.daa_score;
                book.expire_v1(now);
                // Discover: the producer's pending d0 duties, then the claims this seat signed Valid.
                let mut fresh: Vec<(Hash64, Hash64)> = Vec::new();
                if let Ok((page, _)) = state.staged_tir_preparation_duties_after_v1(signer, now, None, PALW_TIR_PUBLIC_CASES_V1) {
                    fresh.extend(page);
                }
                for (claim, (receipt, _)) in own_receipts {
                    if receipt.verdict != PalwReceiptVerdictV2::Valid || receipt.seat_bond != signer {
                        continue;
                    }
                    if let Some(frame) = palw_tir_public_claim_v1(state, *claim) {
                        fresh.push((*claim, frame.plan_v1().allocation_id));
                    }
                }
                for key in fresh {
                    if book.cases.len() >= PALW_TIR_PUBLIC_CASES_V1 || book.cases.contains_key(&key) {
                        continue;
                    }
                    if preparation_duty_v1(state, now, signer, key.0, key.1).is_none() {
                        continue;
                    }
                    let Some(frame) = palw_tir_public_claim_v1(state, key.0) else { continue };
                    match palw_tir_public_work_book_v1(
                        state,
                        Key::TirPreparation(key.0, key.1),
                        signer,
                        frame.descriptor_v1().maximum_leaves,
                    ) {
                        Ok(work) => {
                            book.cases.insert(
                                key,
                                CaseV1 {
                                    work,
                                    reader: None,
                                    writer: None,
                                    warm: false,
                                    quote: None,
                                    permit: None,
                                    failed: false,
                                    _capture: None,
                                    _publish: None,
                                },
                            );
                        }
                        Err(why) => debug!("[{PALW_PANEL}] TIR frame {}: no preparation lane yet: {why}", key.0),
                    }
                }
                // Advance every live case by at most one step.
                let mut ended: Vec<(Hash64, Hash64)> = Vec::new();
                for (key, case) in book.cases.iter_mut() {
                    let Some(frame) = palw_tir_public_claim_v1(state, key.0) else {
                        ended.push(*key);
                        continue;
                    };
                    let maximum = frame.descriptor_v1().maximum_leaves;
                    let Some(producer) = preparation_duty_v1(state, now, signer, key.0, key.1) else {
                        ended.push(*key);
                        continue;
                    };
                    if case.failed {
                        ended.push(*key);
                        continue;
                    }
                    let snapshot = match palw_tir_public_preparation_snapshot_v1(state, Key::TirPreparation(key.0, key.1), maximum) {
                        Ok(Some(snapshot)) => snapshot,
                        _ => {
                            ended.push(*key);
                            continue;
                        }
                    };
                    let c = snapshot.row.preparation.contract_v1().clone();
                    // A sealed capture: file Ready (the journal survives restarts; the native row decides when it is done).
                    if let Some(sealed) = book.sealed.get(key).filter(|s| s.signer == signer) {
                        let view = (!producer).then(|| Box::new(frame.view_v1(state.staged_tir_owner_count_v1(&key.0))));
                        steps.push(StepV1::Ready { key: *key, snapshot: Box::new(snapshot), view, descriptor: sealed.descriptor });
                        continue;
                    }
                    // A debit in flight: confirm it on the accepted state, or keep re-sending the same exact debit.
                    if case.quote.is_some() && case.permit.is_none() {
                        match case.work.confirm_v1(state, maximum) {
                            Ok(Some(permit)) => {
                                case.permit = Some(permit);
                                steps.push(StepV1::Run { key: *key, snapshot: Box::new(snapshot) });
                            }
                            Ok(None) => {
                                if let Some(action) = case.work.pending_action_v1() {
                                    steps.push(StepV1::Debit {
                                        key: *key,
                                        action,
                                        original_accepted_daa: c.original_accepted_daa,
                                        source_id: c.source_id,
                                        due: c.preparation_until_daa.saturating_sub(1),
                                    });
                                }
                            }
                            Err(why) => {
                                debug!("[{PALW_PANEL}] TIR frame {}: preparation debit refused on confirmation: {why}", key.0);
                                case.failed = true;
                            }
                        }
                        continue;
                    }
                    if case.reader.is_none() {
                        steps.push(StepV1::Open {
                            key: *key,
                            header: Box::new(frame.header_v1().clone()),
                            job: Box::new(frame.original_v1().job.clone()),
                            producer,
                        });
                        continue;
                    }
                    if case.warm && case.writer.is_none() {
                        steps.push(StepV1::Begin { key: *key, material_until_daa: c.material_until_daa, now });
                        continue;
                    }
                    // The next slice's quote, then its exact debit.
                    let quote = if case.warm {
                        case.writer.as_ref().map(|writer| writer.slice_quote_v1())
                    } else {
                        case.reader.as_ref().map(|reader| reader.slice_quote_v1(&PalwTirPublicRequestV1::StepLeaf { index: 0 }))
                    };
                    let Some(Ok(quote)) = quote else {
                        case.failed = true;
                        continue;
                    };
                    let work =
                        match preparation_slice_work_v1(&quote).and_then(|w| PALW_TIR_PUBLIC_SLICE_OVERHEAD_V1.checked_add_v1(w)) {
                            Ok(work) => work,
                            Err(why) => {
                                debug!("[{PALW_PANEL}] TIR frame {}: slice work does not price: {why}", key.0);
                                case.failed = true;
                                continue;
                            }
                        };
                    let scope = PalwTirPublicWorkScopeV1 {
                        claim: c.claim,
                        owner: c.producer,
                        reserved_daa: c.original_accepted_daa,
                        bundle_id: c.allocation_id,
                        source_id: c.source_id,
                        turn: 0,
                        request: PalwTirPublicRequestV1::StepLeaf { index: 0 },
                        actor: signer,
                        phase: PalwTirPublicWorkPhaseV1::Capture,
                        input_head: c.source_id,
                        input_offset: 0,
                        slice: quote.sequence,
                        cursor_id: quote.cursor_id,
                    };
                    match case.work.prepare_v1(state, scope, work, None, maximum) {
                        Ok(action) => {
                            case.quote = Some(quote);
                            steps.push(StepV1::Debit {
                                key: *key,
                                action,
                                original_accepted_daa: c.original_accepted_daa,
                                source_id: c.source_id,
                                due: c.preparation_until_daa.saturating_sub(1),
                            });
                        }
                        Err(why) => {
                            debug!("[{PALW_PANEL}] TIR frame {}: the preparation lane refuses the next slice: {why}", key.0);
                            case.failed = true;
                        }
                    }
                }
                for key in ended {
                    book.cases.remove(&key);
                }
                Ok(())
            };
            session.palw_tir_public_read_v1(&mut read)?
        };
        if !armed {
            book.cases.clear();
            pending.retain(|(_, round, responder, _)| !ours(*round, *responder));
            return Ok(());
        }
        // A queued item of a frame this node no longer prepares (Ready accepted, duty ended) is stale.
        pending.retain(|(claim, round, responder, _)| {
            !ours(*round, *responder) || book.cases.keys().any(|(c, _)| c == claim) || book.sealed.keys().any(|(c, _)| c == claim)
        });

        // ---- Act on the read: backend IO off the async thread, then sign and queue. ----
        for step in steps {
            let result: Result<(), String> = async {
                match step {
                    StepV1::Open { key, header, job, producer } => {
                        let class = header.job_context.shape_profile_id;
                        let backend: Arc<dyn kaspa_consensus_core::palw_backend::PalwExecutionBackendV1> = if producer {
                            self.executor_backend_v1(session, class, header.artifact_root)?
                        } else {
                            Arc::from(self.resolve_backend(session, class, header.artifact_root)?)
                        };
                        let paths = self.fp_retained_payload_paths(&key.0);
                        let cache = self.config.retention_dir.join("tir-public-prepared");
                        let (reader, need) = tokio::task::spawn_blocking(move || {
                            let source = retained_original_v1(&paths, &job)?;
                            let need = backend.tir_public_capture_need_v1(&header, source.len_v1())?;
                            std::fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
                            let reader = backend.open_tir_public_capture_v1(&header, source, &cache)?;
                            Ok::<_, String>((reader, need))
                        })
                        .await
                        .map_err(|e| format!("TIR custodian reader worker failed: {e}"))??;
                        let memory = crate::palw_memory_ledger::host_ledger_v1()
                            .reserve(
                                crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
                                    role: "public-tir-custodian-capture",
                                    class_id: class,
                                    job: key.0,
                                },
                                need.resident_ram_bytes,
                            )
                            .map_err(|e| e.to_string())?;
                        if let Some(case) = book.cases.get_mut(&key) {
                            case.reader = Some(reader);
                            case._capture = Some(memory);
                        }
                    }
                    StepV1::Begin { key, material_until_daa, now } => {
                        let Some(case) = book.cases.get_mut(&key) else { return Ok(()) };
                        let reader = case.reader.as_ref().ok_or("TIR custodian lost its warm reader")?;
                        let need = reader.prepared_need_v1()?;
                        let memory = crate::palw_memory_ledger::host_ledger_v1()
                            .reserve(
                                crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
                                    role: "public-tir-custodian-publish",
                                    class_id: Hash64::default(),
                                    job: key.0,
                                },
                                need.additional_ram_bytes,
                            )
                            .map_err(|e| e.to_string())?;
                        let directory = self.config.retention_dir.join("tir-public-prepared");
                        let writer = reader.begin_prepared_v1(&directory, material_until_daa, now)?;
                        case.writer = Some(writer);
                        case._publish = Some(memory);
                    }
                    StepV1::Debit { key, action, original_accepted_daa, source_id, due: due_daa } => {
                        let queue = (key.0, PALW_TIR_PUBLIC_DEBIT_ROUND_V1, true);
                        if pending.iter().any(|(a, b, c, _)| (*a, *b, *c) == queue)
                            || moved.get(&queue).is_some_and(|at| now < at.saturating_add(COURT_MOVE_REPLAN_DAA))
                        {
                            return Ok(());
                        }
                        let control = PalwTirPublicControlV1 {
                            version: 1,
                            claim: key.0,
                            allocation_id: key.1,
                            source_id,
                            target: PalwTirPublicControlTargetV1::Preparation { original_accepted_daa },
                            expected_turn: 0,
                            declared_rent_sompi: 0,
                            signer,
                            action: PalwTirPublicControlActionV1::Preparation(action),
                            signature: Vec::new(),
                        };
                        let object = signed_control_v1(self, network_domain, control)?;
                        pending.push((queue.0, queue.1, queue.2, object));
                        due.insert(queue, due_daa);
                        moved.insert(queue, now);
                    }
                    StepV1::Run { key, snapshot } => {
                        let Some(case) = book.cases.get_mut(&key) else { return Ok(()) };
                        let (Some(quote), Some(permit)) = (case.quote.take(), case.permit.take()) else {
                            return Err("TIR custodian has no accepted permit for its quoted slice".into());
                        };
                        // The debit is accepted: its queued carrier (if still queued) is spent.
                        pending.retain(|(a, b, c, _)| (*a, *b, *c) != (key.0, PALW_TIR_PUBLIC_DEBIT_ROUND_V1, true));
                        let expected = *permit.scope_v1();
                        let mut ledger = OperationalLedgerV1::from_native_permit_v1(permit, expected)?;
                        let warm = case.warm;
                        let mut reader = case.reader.take();
                        let mut writer = case.writer.take();
                        let outcome = tokio::task::spawn_blocking(move || {
                            let borrowed = BorrowedPreparationSnapshotV1::from_native_v1(&snapshot);
                            let result = if warm {
                                match writer.as_mut() {
                                    Some(writer) => {
                                        prepare_claim_publication_slice_v1(&borrowed, writer.as_mut(), &quote, &mut ledger)
                                            .map(|sealed| (true, sealed))
                                    }
                                    None => Err("TIR custodian lost its prepared writer".into()),
                                }
                            } else {
                                match reader.as_mut() {
                                    Some(reader) => prepare_claim_capture_slice_v1(&borrowed, reader.as_mut(), &quote, &mut ledger)
                                        .map(|warmed| (warmed, None)),
                                    None => Err("TIR custodian lost its capture reader".into()),
                                }
                            };
                            (reader, writer, result)
                        })
                        .await
                        .map_err(|e| format!("TIR custodian slice worker failed: {e}"))?;
                        let (reader, writer, result) = outcome;
                        case.reader = reader;
                        case.writer = writer;
                        match result {
                            Ok((warmed, sealed)) => {
                                case.warm |= warmed;
                                if let Some(descriptor) = sealed {
                                    let entry = PalwTirPublicSealedV1 {
                                        claim: key.0,
                                        allocation_id: key.1,
                                        signer,
                                        material_until_daa: descriptor.material_until_daa,
                                        descriptor,
                                    };
                                    book.seal_v1(entry)?;
                                    book.persist();
                                    info!(
                                        "[{PALW_PANEL}] TIR frame {}: prepared capture sealed ({} bytes)",
                                        key.0, descriptor.retained_bytes
                                    );
                                }
                            }
                            Err(why) => {
                                case.failed = true;
                                return Err(why);
                            }
                        }
                    }
                    StepV1::Ready { key, snapshot, view, descriptor } => {
                        let queue = (key.0, PALW_TIR_PUBLIC_READY_ROUND_V1, true);
                        if pending.iter().any(|(a, b, c, _)| (*a, *b, *c) == queue)
                            || moved.get(&queue).is_some_and(|at| now < at.saturating_add(COURT_MOVE_REPLAN_DAA))
                        {
                            return Ok(());
                        }
                        let c = snapshot.row.preparation.contract_v1();
                        let deadline = c.preparation_until_daa.saturating_sub(1);
                        let object = match view {
                            // The producer: its own PreparationReady on the claim-wide lane.
                            None => {
                                let borrowed = BorrowedPreparationSnapshotV1::from_native_v1(&snapshot);
                                let ready = preparation_ready_action_v1(&borrowed, signer, descriptor)?;
                                let control = PalwTirPublicControlV1 {
                                    version: 1,
                                    claim: ready.claim,
                                    allocation_id: ready.allocation_id,
                                    source_id: ready.source_id,
                                    target: PalwTirPublicControlTargetV1::Preparation {
                                        original_accepted_daa: c.original_accepted_daa,
                                    },
                                    expected_turn: ready.expected_turn,
                                    declared_rent_sompi: 0,
                                    signer,
                                    action: PalwTirPublicControlActionV1::Preparation(ready.action),
                                    signature: Vec::new(),
                                };
                                signed_control_v1(self, network_domain, control)?
                            }
                            // A drawn seat: its Valid credit + raw custody grant + Ready, atomically, over its own signed Valid.
                            Some(view) => {
                                let (receipt, _) = own_receipts.get(&key.0).ok_or("the seat's own Valid receipt is gone")?;
                                super::tir_public_filer::palw_tir_public_grant_ready_object_v1(
                                    |message, context| self.sign(message, context).unwrap_or_default(),
                                    network_domain,
                                    &view,
                                    signer,
                                    receipt.signed_daa,
                                    descriptor,
                                )?
                            }
                        };
                        pending.push((queue.0, queue.1, queue.2, object));
                        due.insert(queue, deadline);
                        moved.insert(queue, now);
                    }
                }
                Ok(())
            }
            .await;
            if let Err(why) = result {
                debug!("[{PALW_PANEL}] TIR custodian preparation remains pending: {why}");
            }
        }
        book.persist();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn descriptor(n: u64) -> PalwTirPublicPreparedDescriptorV1 {
        PalwTirPublicPreparedDescriptorV1 {
            version: 1,
            source_id: Hash64::from_u64_word(n),
            class_id: Hash64::from_u64_word(2),
            artifact_root: Hash64::from_u64_word(3),
            execution_root: Hash64::from_u64_word(4),
            trace_root: Hash64::from_u64_word(5),
            step_root: Hash64::from_u64_word(6),
            step_leaf_count: 64,
            decode_count: 8,
            vocab: 256,
            material_until_daa: 900,
            manifest_digest: Hash64::from_u64_word(7),
            retained_bytes: 4096,
        }
    }
    fn sealed(n: u64, until: u64) -> PalwTirPublicSealedV1 {
        PalwTirPublicSealedV1 {
            claim: Hash64::from_u64_word(100 + n),
            allocation_id: Hash64::from_u64_word(200 + n),
            signer: PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(Hash64::from_u64_word(9), 0)),
            material_until_daa: until,
            descriptor: descriptor(n),
        }
    }

    /// The journal survives a restart byte for byte; expired entries leave; a corrupt journal is set aside, never trusted.
    #[test]
    fn sealed_journal_round_trips_expires_and_sets_aside_a_corrupt_file() {
        let dir = tempfile::tempdir().unwrap();
        let mut book = PalwTirPublicCustodianBookV1::load(dir.path());
        book.seal_v1(sealed(1, 500)).unwrap();
        book.seal_v1(sealed(2, 900)).unwrap();
        book.persist();
        let mut reloaded = PalwTirPublicCustodianBookV1::load(dir.path());
        assert_eq!(reloaded.sealed, book.sealed);
        reloaded.expire_v1(600);
        assert!(reloaded.sealed_v1(Hash64::from_u64_word(101), Hash64::from_u64_word(201)).is_none());
        assert_eq!(reloaded.sealed_v1(Hash64::from_u64_word(102), Hash64::from_u64_word(202)), Some(&sealed(2, 900)));
        reloaded.persist();
        assert_eq!(PalwTirPublicCustodianBookV1::load(dir.path()).sealed.len(), 1);
        std::fs::write(dir.path().join("palw-tir-public-custodian.v1"), b"not a journal").unwrap();
        assert!(PalwTirPublicCustodianBookV1::load(dir.path()).sealed.is_empty());
        assert!(dir.path().join("palw-tir-public-custodian.v1.unreadable").exists());
    }

    /// The accepted debit covers the driver's fixed per-slice overhead plus the slice's own quoted work.
    #[test]
    fn slice_debit_covers_the_fixed_overhead_and_the_quote() {
        let quote = PalwTirPublicCaptureSliceV1 {
            source_id: Hash64::from_u64_word(1),
            request_id: Hash64::default(),
            sequence: 3,
            phase: 12,
            cursor_id: Hash64::from_u64_word(4),
            work: kaspa_consensus_core::palw_backend::PalwTirPublicCaptureWorkV1 {
                io_bytes: 16_384,
                hash_bytes: 16_384,
                ..Default::default()
            },
            resident_ram_bytes: 0,
        };
        let work = PALW_TIR_PUBLIC_SLICE_OVERHEAD_V1.checked_add_v1(preparation_slice_work_v1(&quote).unwrap()).unwrap();
        assert_eq!(work.attempts, 2);
        assert_eq!(work.hash_bytes, 8192 + 16_384);
        assert_eq!(work.parser_steps, 1024 + 16 * 16_384);
        assert_eq!(work.io_bytes, 16_384);
    }
}
