//! **RFC-0006 — layer-sharded panels, the node's half.**
//!
//! A claim of an IR class that declared a layer-shard plan binds a panel drawn per shard (consensus,
//! `palw_tir_shard_v1`). A seat of such a panel owes one shard, not the class: it verifies the CELLS of its slice — its layer
//! range crossed with its assigned position segments — from the claim's committed boundary rows and only its shard's weights
//! (`misaka_palw_tir_exec::node::cell`), and files a cell-masked receipt (`PalwSeatReceiptV4`). This module is everything the
//! panel loop does about it:
//!
//! * [`PalwTirShardBooksV1`] — the loop's bookkeeping for sharded claims: the V4 receipts heard (admitted against the tip's
//!   panel and the seat's registered key, one place per `(claim, shard, bond)`), this seat's own filings and their re-send,
//!   what it answered, the accusations its cells found, what it carried;
//! * [`palw_tir_shard_outcome_v1`] — the pure verdict of one duty over a capture; [`palw_tir_shard_accusation_v1`] — the IR
//!   one-move accusation a finding builds (the same `TirShardCourtAccused` object a whole-job replay builds, from the leaf
//!   the cell found: the upstream cell's boundary row for a consistent lie, the lie's own leaf for an in-cell one);
//! * the seat pass ([`PalwPanelService::tir_shard_seat_pass_v1`]), the part collector
//!   ([`PalwTirShardBooksV1::parts_to_offer`]) and the carriage of the node's own objects ([`PalwPanelService::tir_shard_objects_v1`]:
//!   the registrant's plan declaration and each held shard's possession proof).
//!
//! **Shadow mode** (`--palw-tir-shard-shadow`): the whole pass runs — cells verified, verdicts logged — and nothing is signed,
//! gossiped, accused or carried: the node-only period RFC-0006 decision 1 puts before the fence is armed. **A device**
//! (`--palw-tir-shard-gpu`) is a plug-in the binary registers ([`misaka_palw_sdk::lineages::tir::register_device_v1`]); a
//! node that asked for one and has none says so once and runs the CPU — a refusal of the device is never a different verdict.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_panel_v2::{PalwReceiptVerdictV2, PalwSeatReceiptV2};
use kaspa_consensus_core::palw_producer_v2::{PalwDisputableClaimV2, PalwSeatDutyV2};
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2};
use kaspa_consensus_core::palw_tir_court_v1::PalwTirCourtRulesV1;
use kaspa_consensus_core::palw_tir_one_move_v1::PalwTirOneMoveAccusationV1;
use kaspa_consensus_core::palw_tir_shard_v1::{
    PALW_RECEIPT_V4_MLDSA87_CONTEXT, PALW_TIR_SHARD_PLAN_MLDSA87_CONTEXT, PALW_TIR_SHARD_READINESS_MLDSA87_CONTEXT, PalwSeatReceiptV4,
    palw_receipt_message_v4, palw_tir_shard_inventory_ranges_v1, palw_tir_shard_plan_message_v1, palw_tir_shard_readiness_leaves_v1,
    palw_tir_shard_readiness_message_v1, palw_tir_shard_ready_class_v1,
};
use kaspa_core::{info, warn};
use misaka_palw_sdk::lineages::tir::{
    KernelBackendV1, TirBackendV1, TirCellVerdictV1, cell_runs_v1, tir_kernel_backend_registered_v1, tir_kernel_backend_v1, tir_shard_cells_v1,
    tir_shard_geometry_v1, tir_shard_weight_bytes_v1, tir_verify_capture_cells_v1,
};

use super::tir_court::{
    PalwTirCloseCandidateV1, PalwTirOneMoveCaseV1, palw_tir_close_candidates_v1, palw_tir_duty_target_v1,
    palw_tir_one_move_accusation_to_file_v1, palw_tir_one_move_case_at_dissected_leaf_v1,
};
use super::{PALW_PANEL, SeatDutyPanelKeyV1, seat_duty_panel_key_v1};
use crate::palw_receipt_pool::ReceiptChainFactsV1;

/// How many cell-masked receipts a node holds at most: heard ones queue here until the next tick's read of the tip.
pub(crate) const PALW_TIR_SHARD_ARRIVALS_MAX_V1: usize = 512;

/// The ledger role a sharded duty's cells reserve under, and the scratch a cell run needs beyond the capture's bytes.
pub(crate) const PALW_TIR_SHARD_CELL_ROLE_V1: &str = "tir-shard-cell";
pub(crate) const PALW_TIR_SHARD_CELL_SCRATCH_BYTES_V1: u64 = 16 << 20;

/// How many claims' receipts the pool keeps beside the claims this node's duties and filings name.
pub(crate) const PALW_TIR_SHARD_POOL_CLAIMS_V1: usize = 128;

/// How many DAA an own possession proof or plan declaration is left alone after it was carried, before it is carried again.
pub(crate) const PALW_TIR_SHARD_CARRY_REPLAN_DAA_V1: u64 = 30;

// ---------------------------------------------------------------------------------------------
// The verdict of a duty (pure over a capture)
// ---------------------------------------------------------------------------------------------

/// **What a seat found in its cells.**
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PalwTirShardOutcomeV1 {
    /// Every leaf of every cell the duty names is the program's function of the committed inputs: sign `Valid`.
    Valid { leaves: u64, positions: u32 },
    /// A cell's recomputation parts from the committed leaf (`leaf`), or a generated id is not the greedy selection over its
    /// committed row (`row`): the accusation's input. `position` is the position the finding is at.
    Fault { leaf: Option<u64>, row: Option<u32>, position: u32 },
    /// The seat could not run the duty (no capture, a fold, a shard it does not hold, an input the capture lacks): it signs
    /// nothing from the cell and falls back to the abstention rules.
    Abstain(String),
}

/// **A duty's verdict over the claim's dense capture**: the duty's cells (its shard, its assigned segments) run on `device`
/// (the CPU when it refuses). Pure over its inputs.
pub(crate) fn palw_tir_shard_outcome_v1(
    tir: &TirBackendV1,
    material: &[u8],
    duty: &PalwSeatDutyV2,
    device: &mut dyn KernelBackendV1,
) -> Result<PalwTirShardOutcomeV1, String> {
    let place = duty.tir_shard.ok_or("the duty is a flat panel's")?;
    let capture = tir.decode_capture(material)?;
    if !capture.is_dense() {
        return Ok(PalwTirShardOutcomeV1::Abstain("the material is a fold: it carries no committed rows".to_string()));
    }
    let ctx = &capture.binding.job_context;
    let positions = tir.space().job_shape(ctx).map_err(|e| e.to_string())?.positions;
    let cells = tir_shard_cells_v1(tir, positions, place.shard, place.s_l, place.s_p, place.segments)?;
    Ok(match tir_verify_capture_cells_v1(tir, material, &cells, device)? {
        TirCellVerdictV1::Verified { leaves, positions } => PalwTirShardOutcomeV1::Valid { leaves, positions },
        TirCellVerdictV1::Faulted { leaf, position } => PalwTirShardOutcomeV1::Fault { leaf: Some(leaf), row: None, position },
        TirCellVerdictV1::TokenFault { position } => {
            // Position `a` selects row `a + 1 − prefill` of the decode trace.
            let row = (u64::from(position) + 1).checked_sub(u64::from(ctx.declared_prefill_tokens)).and_then(|r| u32::try_from(r).ok());
            PalwTirShardOutcomeV1::Fault { leaf: None, row, position }
        }
        TirCellVerdictV1::Unavailable { leaf } => {
            PalwTirShardOutcomeV1::Abstain(format!("the capture does not carry leaf {leaf} the duty's cells read"))
        }
        TirCellVerdictV1::Refused(why) => PalwTirShardOutcomeV1::Abstain(why),
    })
}

/// **The IR one-move accusation a finding builds** — the first close the court convicts on, at the leaf the cell found (its
/// cone, or the named-leaf challenge at a dissected leaf) or at the decode row whose id is not the greedy selection (the
/// decode-token door, the seat's own selection the lane that beats the committed one). `None` when no close convicts: nothing is
/// filed, because a one-move accusation that does not convict charges its accuser.
#[allow(clippy::too_many_arguments)]
pub(crate) fn palw_tir_shard_accusation_v1(
    tir: &TirBackendV1,
    accused: &[u8],
    leaf: Option<u64>,
    row: Option<u32>,
    target: &PalwDisputableClaimV2,
    accuser: PalwBondKeyV2,
    court: &kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2,
    rules: &PalwTirCourtRulesV1,
    ladder: u64,
    form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
) -> Result<Option<(&'static str, PalwTirOneMoveAccusationV1)>, String> {
    let mut candidates: Vec<PalwTirCloseCandidateV1> = match leaf {
        Some(leaf) => palw_tir_close_candidates_v1(tir, accused, None, leaf, rules, false),
        None => Vec::new(),
    };
    if let Some(row) = row {
        let capture = tir.decode_capture(accused)?;
        if let Some(lanes) = capture.logits_rows.get(row as usize) {
            let own = kaspa_consensus_core::palw_step_refute::base0_decode_token_select_v1(lanes) as u32;
            let mut door = tir.decode_token_close(accused, row, own);
            if let Ok(proof) = door.as_mut() {
                proof.tir_strip_program_v1();
            }
            candidates.push(("decode token", door));
        }
    }
    let (case, _dissected) =
        palw_tir_one_move_case_at_dissected_leaf_v1(tir, accused, PalwTirOneMoveCaseV1 { leaf, row, candidates }, rules);
    let program = tir.class().program.clone();
    Ok(palw_tir_one_move_accusation_to_file_v1(case.candidates, target, &program, accuser, court, ladder, form))
}

// ---------------------------------------------------------------------------------------------
// The wire: V4 receipts
// ---------------------------------------------------------------------------------------------

/// **Queue one cell-masked receipt off the wire** for the next tick's admission. V4 first: a V4 receipt is a V2 one with a shard
/// and a mask after it, and borsh refuses trailing bytes, so no other version decodes as it and it decodes as no other.
/// What the drain refuses without the chain: bytes that are no V4 receipt and a signature that is not an ML-DSA-87 signature's
/// length. Returns `true` when the bytes were a V4 receipt (queued or refused as junk) — the caller stops there.
pub(crate) fn palw_tir_shard_arrival_push_v1(queue: &mut VecDeque<PalwSeatReceiptV4>, bytes: &[u8]) -> bool {
    let Ok(receipt) = borsh::from_slice::<PalwSeatReceiptV4>(bytes) else { return false };
    if receipt.receipt.signature.len() != kaspa_txscript::MLDSA87_SIG_LEN {
        return true;
    }
    if queue.len() >= PALW_TIR_SHARD_ARRIVALS_MAX_V1 {
        queue.pop_front();
    }
    queue.push_back(receipt);
    true
}

/// What admission did with one arrival.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PalwTirShardAdmitV1 {
    Kept,
    /// The same seat's receipt for this shard is already pooled (a re-send, re-signed).
    Redundant,
    NoPanel,
    NotOnPanel,
    Stale,
    UnknownBond,
    BadSignature,
    /// The pool is at its claim ceiling and the claim is not one this node keeps.
    Full,
}

/// One own receipt, for the re-send.
#[derive(Clone, Debug)]
pub(crate) struct PalwTirShardOwnFiledV1 {
    pub verdict: PalwReceiptVerdictV2,
    pub signed_daa: u64,
    pub shard: u16,
    pub segments: kaspa_consensus_core::palw_verification_v2::PalwSegmentMaskV2,
    pub schedule: crate::palw_receipt_pool::OwnRebroadcastV1,
}

/// An accusation a cell found, built off the tick and waiting for the court's carrier path.
#[derive(Clone, Debug)]
pub(crate) struct PalwTirShardFindingV1 {
    pub target: PalwDisputableClaimV2,
    pub due: u64,
    pub label: &'static str,
    pub leaf: Option<u64>,
    pub row: Option<u32>,
    pub accusation: PalwTirOneMoveAccusationV1,
}

/// **The panel loop's bookkeeping for layer-sharded claims.**
#[derive(Default)]
pub(crate) struct PalwTirShardBooksV1 {
    /// Cell-masked receipts heard, waiting for this tick's chain read.
    pub arrivals: VecDeque<PalwSeatReceiptV4>,
    /// The admitted receipts, one per `(claim, shard, seat bond)`.
    pool: BTreeMap<(Hash64, u16), BTreeMap<PalwBondKeyV2, PalwSeatReceiptV4>>,
    /// The tip's panels and the seats' registered keys, as the pools' own facts.
    facts: ReceiptChainFactsV1,
    /// This seat's own filings by `(claim, shard, seat index)`, for the re-send while the duty stands.
    pub filed: HashMap<(Hash64, u16, u8), PalwTirShardOwnFiledV1>,
    /// Duties answered (a receipt filed, or a finding raised): once per panel place.
    pub answered: HashSet<SeatDutyPanelKeyV1>,
    /// When this seat first wanted a claim's material (the abstention's `requested_daa`).
    first_wanted: HashMap<Hash64, u64>,
    /// Findings waiting to ride the court's carrier path.
    pub findings: Vec<PalwTirShardFindingV1>,
    /// `TirStepRun` demands (`DefaultAccusedTirStep`) waiting for the court's carrier path, with their messages and due DAAs.
    pub demands: Vec<(Hash64, PalwConsensusObjectV2, u64)>,
    /// Claims this seat has demanded the runs of (once).
    demanded: HashSet<Hash64>,
    /// Claims whose cells this seat refuted (or could not accuse): never answered `Valid`.
    pub refuted: HashSet<Hash64>,
    /// `(claim)` → the DAA a part was last carried.
    submitted: HashMap<Hash64, u64>,
    /// `(class, shard)` → the span a possession proof was last carried; the plan declaration's DAA by class.
    carried_proof: HashMap<(Hash64, u16), (u64, u64)>,
    carried_plan: HashMap<Hash64, u64>,
    /// The device was asked for and none was registered: said once.
    said_no_device: bool,
    /// Cells verified and leaves recomputed since start, for the node's status line.
    pub cells_verified: u64,
    pub leaves_recomputed: u64,
}

impl PalwTirShardBooksV1 {
    /// This seat's own receipt, kept apart from what can be evicted by an arrival: put straight in the pool and the re-send table.
    pub(crate) fn insert_own(&mut self, receipt: PalwSeatReceiptV4, seat_index: u8, now: std::time::Instant) {
        self.filed.insert(
            (receipt.receipt.claim, receipt.shard, seat_index),
            PalwTirShardOwnFiledV1 {
                verdict: receipt.receipt.verdict,
                signed_daa: receipt.receipt.signed_daa,
                shard: receipt.shard,
                segments: receipt.segments,
                schedule: crate::palw_receipt_pool::OwnRebroadcastV1::filed(now),
            },
        );
        self.pool.entry((receipt.receipt.claim, receipt.shard)).or_default().insert(receipt.receipt.seat_bond, receipt);
    }

    /// **Admit this tick's arrivals** against one read of the tip: the panel must be bound and name the seat, the receipt must not
    /// predate the bind, and its signature must verify under the key the seat's bond registered. One place per
    /// `(claim, shard, bond)`: a re-sent receipt of a seat already pooled adds nothing. `keep` names the claims the loop keeps
    /// whatever the pool's ceiling says (its duties and filings).
    pub(crate) fn admit_arrivals(
        &mut self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        network_domain: Hash64,
        keep: &HashSet<Hash64>,
    ) -> Vec<PalwTirShardAdmitV1> {
        if self.arrivals.is_empty() {
            return Vec::new();
        }
        let arrivals: Vec<PalwSeatReceiptV4> = self.arrivals.drain(..).collect();
        let mut claims: Vec<Hash64> = self.pool.keys().map(|(c, _)| *c).chain(arrivals.iter().map(|r| r.receipt.claim)).collect();
        claims.sort_unstable();
        claims.dedup();
        let referenced: HashSet<PalwBondKeyV2> =
            self.pool.values().flat_map(|m| m.keys().copied()).chain(arrivals.iter().map(|r| r.receipt.seat_bond)).collect();
        let mut bonds: Vec<PalwBondKeyV2> = referenced.iter().filter(|b| self.facts.needs_key(b)).copied().collect();
        bonds.sort_unstable();
        let Some(tip) = session.palw_receipt_pool_facts_v1(claims, bonds) else {
            // No read: the arrivals wait for the next tick, held to the queue's ceiling.
            self.arrivals.extend(arrivals.into_iter().take(PALW_TIR_SHARD_ARRIVALS_MAX_V1));
            return Vec::new();
        };
        self.facts.refresh(tip, &referenced);
        let mut arrivals = arrivals;
        arrivals.sort_by_key(|r| !keep.contains(&r.receipt.claim));
        let outcomes = arrivals.into_iter().map(|r| self.admit_one(r, network_domain, keep)).collect();
        // Prune to the panels the tip holds: a claim whose panel is gone (voided, final) is no longer anybody's duty.
        let panels: HashSet<Hash64> = self.pool.keys().map(|(c, _)| *c).filter(|c| self.facts.panel(c).is_some()).collect();
        self.pool.retain(|(claim, _), _| panels.contains(claim));
        outcomes
    }

    fn admit_one(&mut self, receipt: PalwSeatReceiptV4, network_domain: Hash64, keep: &HashSet<Hash64>) -> PalwTirShardAdmitV1 {
        use PalwTirShardAdmitV1 as A;
        let (claim, bond) = (receipt.receipt.claim, receipt.receipt.seat_bond);
        let Some(panel) = self.facts.panel(&claim) else { return A::NoPanel };
        if !panel.seats.contains(&bond) {
            return A::NotOnPanel;
        }
        if receipt.receipt.signed_daa < panel.bound_daa {
            return A::Stale;
        }
        let Some(key) = self.facts.seat_key(&bond) else { return A::UnknownBond };
        if self.pool.get(&(claim, receipt.shard)).is_some_and(|m| m.contains_key(&bond)) {
            return A::Redundant;
        }
        let claims_held = self.pool.keys().map(|(c, _)| *c).collect::<HashSet<_>>();
        if !claims_held.contains(&claim) && !keep.contains(&claim) && claims_held.len() >= PALW_TIR_SHARD_POOL_CLAIMS_V1 {
            return A::Full;
        }
        let message = palw_receipt_message_v4(
            network_domain,
            claim,
            receipt.receipt.verdict,
            receipt.receipt.signed_daa,
            receipt.shard,
            receipt.segments,
        );
        if !super::verify_receipt_signature_v1(&key, message.as_byte_slice(), &receipt.receipt.signature, PALW_RECEIPT_V4_MLDSA87_CONTEXT)
        {
            return A::BadSignature;
        }
        self.pool.entry((claim, receipt.shard)).or_default().insert(bond, receipt);
        A::Kept
    }

    /// **The parts this node can offer now**, one per claim: the V4 receipts pooled for the claim go to the consensus assembler
    /// (`palw_v2_tir_shard_part_assemble`), which returns the next shard whose set the acceptance validator itself licenses. A claim
    /// offered within [`super::COURT_MOVE_REPLAN_DAA`] is left alone.
    pub(crate) fn parts_to_offer(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        current_daa: u64,
    ) -> Vec<(Hash64, PalwConsensusObjectV2)> {
        let claims: Vec<Hash64> = {
            let mut c: Vec<Hash64> = self.pool.keys().map(|(c, _)| *c).collect();
            c.dedup();
            c
        };
        let mut out = Vec::new();
        for claim in claims {
            if self.submitted.get(&claim).is_some_and(|at| current_daa < at.saturating_add(super::COURT_MOVE_REPLAN_DAA)) {
                continue;
            }
            let candidates: Vec<PalwSeatReceiptV4> =
                self.pool.range((claim, 0)..=(claim, u16::MAX)).flat_map(|(_, m)| m.values().cloned()).collect();
            if candidates.is_empty() {
                continue;
            }
            if let Some(object) = session.palw_v2_tir_shard_part_assemble(claim, candidates) {
                out.push((claim, object));
            }
        }
        out
    }

    /// A part of `claim` was carried at `current_daa`.
    pub(crate) fn note_part_carried(&mut self, claim: Hash64, current_daa: u64) {
        self.submitted.insert(claim, current_daa);
    }

    /// Own receipts due for a re-send now, longest-waiting first, at most `limit`: those whose duty still stands.
    pub(crate) fn own_due(&self, standing: &HashSet<(Hash64, u16, u8)>, now: std::time::Instant, limit: usize) -> Vec<(Hash64, u16, u8)> {
        let mut due: Vec<(Hash64, u16, u8)> =
            self.filed.iter().filter(|(k, f)| standing.contains(k) && f.schedule.due(now)).map(|(k, _)| *k).collect();
        due.sort_unstable();
        due.truncate(limit);
        due
    }
}

// ---------------------------------------------------------------------------------------------
// The service's half
// ---------------------------------------------------------------------------------------------

impl super::PalwPanelService {
    /// Is this node a sharded-panel seat at all: a bond, a key, and — unless the operator turned the whole thing off by naming
    /// no shard — duties to answer.
    fn tir_shard_may_answer_v1(&self, place: &kaspa_consensus_core::palw_producer_v2::PalwTirShardDutyV1) -> bool {
        self.config.tir_shard_hold.is_empty() || self.config.tir_shard_hold.contains(&place.shard)
    }

    /// **The seat pass: answer every sharded duty exactly once.** For each duty not answered yet: the claim's capture (the one
    /// that answers for the claim's own roots) is looked for in the pooled material; a seat without it asks the network (signed)
    /// and, past half the receipt window, files `Unavailable`; with it the duty's cells run off the tick, on the device when one
    /// is registered and asked for, and the verdict is
    ///
    /// * `Valid` → a signed V4 receipt over the shard and the seat's assigned segments, pooled, broadcast and kept for the re-send;
    /// * a finding → no receipt, and the IR one-move accusation its leaf (or decode row) builds, waiting in
    ///   [`PalwTirShardBooksV1::findings`] for the court's carrier path;
    /// * an abstention (a fold, an input the capture lacks, a refused cell) → nothing signed this tick, tried again next.
    ///
    /// In shadow mode the verdict is logged and nothing else happens.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn tir_shard_seat_pass_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        bond_key: PalwBondKeyV2,
        network_domain: Hash64,
        current_daa: u64,
        duties: &[PalwSeatDutyV2],
        materials: &HashMap<Hash64, Vec<Vec<u8>>>,
        books: &mut PalwTirShardBooksV1,
    ) {
        for duty in duties {
            let Some(place) = duty.tir_shard else { continue };
            if !self.tir_shard_may_answer_v1(&place) {
                continue;
            }
            let key = seat_duty_panel_key_v1(duty);
            if books.answered.iter().any(|k| *k == key) || books.refuted.contains(&duty.claim_id) || current_daa > duty.receipt_deadline {
                continue;
            }
            let tir = match self.backends().resolve_tir_v1(duty.class_id, duty.artifact_root) {
                None => continue,
                Some(Ok(tir)) => std::sync::Arc::new(tir),
                Some(Err(why)) => {
                    crate::palw_backends::note_throttled_v1("tir-shard-backend", || {
                        format!("[{PALW_PANEL}] claim {}: the IR backend does not build for its class ({why})", duty.claim_id)
                    });
                    continue;
                }
            };
            // The capture the claim's executor served that answers for the claim's own roots.
            let material = materials
                .get(&duty.claim_id)
                .and_then(|pool| {
                    pool.iter().find(|m| {
                        tir.decode_capture(m).is_ok_and(|c| {
                            c.binding.committed_execution_root == duty.execution_root && c.binding.full_logits_trace_root == duty.trace_root
                        })
                    })
                })
                .cloned();
            let first = *books.first_wanted.entry(duty.claim_id).or_insert(current_daa);
            let shadow_now = self.config.tir_shard_shadow;
            let Some(material) = material else {
                // Ask the network (signed), and abstain honestly once half the window has passed with nothing served.
                self.request_material_signed(network_domain, duty.claim_id, current_daa).await;
                let window = duty.receipt_deadline.saturating_sub(duty.bound_daa);
                // **Past a quarter of the window, with `--palw-tir-shard-demand-runs`: demand the runs the cell reads on chain** (RFC-0006
                // §3, D-S4) — one `TirStepRun` unit, the first run of the seat's first cell; the executor answers inside the
                // disclosure window or its claim defaults. Once a claim.
                if self.config.tir_shard_demand_runs
                    && !shadow_now
                    && !books.demanded.contains(&duty.claim_id)
                    && current_daa >= duty.bound_daa.saturating_add(window / 4)
                {
                    books.demanded.insert(duty.claim_id);
                    if let Some((message, object, due)) = self.tir_shard_run_demand_v1(session, bond_key, network_domain, current_daa, duty, &tir).await {
                        books.demands.push((message, object, due));
                    }
                }
                if current_daa >= duty.bound_daa.saturating_add(window / 2) {
                    let verdict = PalwReceiptVerdictV2::Unavailable { chunk_index: 0, requested_daa: first.max(duty.bound_daa) };
                    self.tir_shard_file_receipt_v1(bond_key, network_domain, current_daa, duty, verdict, books).await;
                    books.answered.insert(key);
                }
                continue;
            };
            let (want_device, shadow) = (self.config.tir_shard_gpu, self.config.tir_shard_shadow);
            if want_device && !books.said_no_device && !tir_kernel_backend_registered_v1() {
                books.said_no_device = true;
                warn!(
                    "[{PALW_PANEL}] --palw-tir-shard-gpu asked for a device backend and this build registered none: cells run on the CPU \
                     (the verdicts are the same)"
                );
            }
            let (task_tir, task_duty, task_material) = (tir.clone(), duty.clone(), material.clone());
            let outcome = tokio::task::spawn_blocking(move || {
                // **The ledger's slots.** The cells' scratch is the host's (the capture's rows and their hashes); a device
                // backend's shard bytes are reserved on its own pool, and a device that cannot take them is not used: the
                // CPU runs the cells (the verdict is the same).
                let key = crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
                    role: PALW_TIR_SHARD_CELL_ROLE_V1,
                    class_id: task_duty.class_id,
                    job: task_duty.claim_id,
                };
                let _host = crate::palw_memory_ledger::host_ledger_v1()
                    .reserve(key.clone(), (task_material.len() as u64).saturating_mul(3).saturating_add(PALW_TIR_SHARD_CELL_SCRATCH_BYTES_V1))
                    .map_err(|refusal| format!("the cells' scratch is not free ({refusal})"))?;
                let (mut device, mut on_device) = tir_kernel_backend_v1(want_device);
                let mut _device_hold = None;
                if on_device {
                    if let Some(capacity) = device.device_capacity_bytes() {
                        crate::palw_memory_ledger::arm_device_share_v1(0, capacity);
                    }
                    let place = task_duty.tir_shard.ok_or("the duty is a flat panel's")?;
                    let shard_bytes = tir_shard_weight_bytes_v1(&task_tir, place.shard, place.s_l)?;
                    match crate::palw_memory_ledger::device_ledger_v1(0).map(|l| l.reserve(key, u64::try_from(shard_bytes).unwrap_or(u64::MAX))) {
                        Some(Ok(held)) => _device_hold = Some(held),
                        _ => {
                            on_device = false;
                            device = Box::new(misaka_palw_sdk::lineages::tir::CpuKernelBackendV1);
                        }
                    }
                }
                let _ = on_device;
                palw_tir_shard_outcome_v1(&task_tir, &task_material, &task_duty, device.as_mut())
            })
            .await;
            let outcome = match outcome {
                Ok(Ok(outcome)) => outcome,
                Ok(Err(why)) => {
                    crate::palw_backends::note_throttled_v1(&format!("tir-shard-cells-{}", duty.claim_id), || {
                        format!("[{PALW_PANEL}] claim {} shard {}: the cells do not run ({why})", duty.claim_id, place.shard)
                    });
                    continue;
                }
                Err(_) => continue,
            };
            match outcome {
                PalwTirShardOutcomeV1::Valid { leaves, positions } => {
                    books.cells_verified += 1;
                    books.leaves_recomputed += leaves;
                    info!(
                        "[{PALW_PANEL}] claim {} shard {}/{} (segments {:#x}{}): every cell verifies ({leaves} leaves over {positions} positions){}",
                        duty.claim_id,
                        place.shard,
                        place.s_l,
                        place.segments.0,
                        if place.outsider { ", the shard's outsider" } else { "" },
                        if shadow { " — shadow: nothing filed" } else { "" }
                    );
                    if !shadow {
                        self.tir_shard_file_receipt_v1(bond_key, network_domain, current_daa, duty, PalwReceiptVerdictV2::Valid, books).await;
                    }
                    books.answered.insert(key);
                }
                PalwTirShardOutcomeV1::Fault { leaf, row, position } => {
                    warn!(
                        "[{PALW_PANEL}] claim {} shard {}/{}: a cell finds the executor's commitment false (leaf {leaf:?}, token row {row:?}, \
                         position {position}){}",
                        duty.claim_id,
                        place.shard,
                        place.s_l,
                        if shadow { " — shadow: no accusation" } else { "" }
                    );
                    books.refuted.insert(duty.claim_id);
                    books.answered.insert(key);
                    if shadow {
                        continue;
                    }
                    let target = palw_tir_duty_target_v1(duty);
                    let earliest_final = super::palw_seat_claim_earliest_final_v1(&self.consensus_config.params, duty.bound_daa);
                    let due = super::palw_seat_court_filing_due_v1(duty.receipt_deadline, earliest_final, current_daa);
                    let court = self.config.court;
                    let network_ladder = kaspa_consensus_core::palw_court_v2::palw_refutation_leaf_cap_v2(
                        &court,
                        self.consensus_config.params.palw_court_ladder.is_some_and(|f| f.is_active(current_daa)),
                    );
                    let ladder = self.seat_refutation_ladder_v1(target.class_id, network_ladder, current_daa);
                    let form = self.config.prompt_ids_form;
                    let mut rules = tir.court_rules(&court);
                    rules.max_step_leaf_count = ladder;
                    let built = {
                        let (tir, accused, target) = (tir.clone(), material.clone(), target.clone());
                        tokio::task::spawn_blocking(move || {
                            palw_tir_shard_accusation_v1(&tir, &accused, leaf, row, &target, bond_key, &court, &rules, ladder, form)
                        })
                        .await
                    };
                    match built {
                        Ok(Ok(Some((label, accusation)))) => {
                            books.findings.push(PalwTirShardFindingV1 { target, due, label, leaf, row, accusation });
                        }
                        Ok(Ok(None)) => warn!(
                            "[{PALW_PANEL}] claim {}: no IR close convicts at the cell's finding; recorded, not filed",
                            duty.claim_id
                        ),
                        Ok(Err(why)) => {
                            warn!("[{PALW_PANEL}] claim {}: the cell's accusation does not build ({why}); recorded, not filed", duty.claim_id)
                        }
                        Err(_) => {}
                    }
                }
                PalwTirShardOutcomeV1::Abstain(why) => {
                    crate::palw_backends::note_throttled_v1(&format!("tir-shard-abstain-{}", duty.claim_id), || {
                        format!("[{PALW_PANEL}] claim {} shard {}: the cells are not run ({why})", duty.claim_id, place.shard)
                    });
                }
            }
        }
        let _ = session;
    }

    /// **The demand of the first run a cell reads** (`DefaultAccusedTirStep`, unit `TirStepRun`), built over the job the claim's block
    /// asked for (the cell's runs are a function of the class, the plan and the job — no capture is needed to list them).
    async fn tir_shard_run_demand_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        bond_key: PalwBondKeyV2,
        network_domain: Hash64,
        current_daa: u64,
        duty: &PalwSeatDutyV2,
        tir: &std::sync::Arc<TirBackendV1>,
    ) -> Option<(Hash64, PalwConsensusObjectV2, u64)> {
        use kaspa_consensus_core::palw_da_rcore_v1::{PalwDaUnitV1, palw_tir_step_accusation_message_v1, palw_tir_step_accusation_object_v1};
        use kaspa_consensus_core::palw_tir_court_v1::PALW_TIR_STEP_RUN_MAX_LEAVES_V1;
        let place = duty.tir_shard?;
        let backend = self.resolve_backend(session, duty.class_id, duty.artifact_root).ok()?;
        let (ctx, _) =
            self.attempt_job_for_claim(session, backend.as_ref(), network_domain, duty.accepted_block, duty.class_id, &duty.executor_bond)?;
        let positions = tir.space().job_shape(&ctx).ok()?.positions;
        let cells = tir_shard_cells_v1(tir, positions, place.shard, place.s_l, place.s_p, place.segments).ok()?;
        let cell = cells.first()?;
        let runs = cell_runs_v1(tir.space(), &ctx, cell).ok()?;
        let (first, count, _) = *runs.first()?;
        let unit = PalwDaUnitV1::TirStepRun { first, count: count.min(PALW_TIR_STEP_RUN_MAX_LEAVES_V1) };
        let ladder = crate::palw_producer::palw_tir_da_answerable_leaves_v1(&self.consensus_config.params, current_daa);
        let message = palw_tir_step_accusation_message_v1(network_domain, &duty.claim_id, &unit, &bond_key);
        let object = palw_tir_step_accusation_object_v1(&network_domain, duty.claim_id, unit, bond_key, ladder, |m, c| self.sign(m, c))
            .map_err(|why| warn!("[{PALW_PANEL}] claim {}: the run demand does not build ({why})", duty.claim_id))
            .ok()?;
        let earliest_final = super::palw_seat_claim_earliest_final_v1(&self.consensus_config.params, duty.bound_daa);
        let due = super::palw_seat_court_filing_due_v1(duty.receipt_deadline, earliest_final, current_daa);
        info!(
            "[{PALW_PANEL}] claim {}: no capture reaches this seat — demanding the run [{first}, +{}) its shard {} cell reads on chain (RFC-0006)",
            duty.claim_id,
            count.min(PALW_TIR_STEP_RUN_MAX_LEAVES_V1),
            place.shard
        );
        Some((message, object, due))
    }

    /// Sign and send one cell-masked receipt for `duty`: pooled as this node's own (never evictable), broadcast, and kept for the
    /// re-send while the duty stands.
    async fn tir_shard_file_receipt_v1(
        &self,
        bond_key: PalwBondKeyV2,
        network_domain: Hash64,
        current_daa: u64,
        duty: &PalwSeatDutyV2,
        verdict: PalwReceiptVerdictV2,
        books: &mut PalwTirShardBooksV1,
    ) {
        let Some(place) = duty.tir_shard else { return };
        let signed_daa = current_daa.clamp(duty.bound_daa, duty.receipt_deadline);
        let message = palw_receipt_message_v4(network_domain, duty.claim_id, verdict, signed_daa, place.shard, place.segments);
        let Some(signature) = self.sign(message.as_byte_slice(), PALW_RECEIPT_V4_MLDSA87_CONTEXT) else { return };
        let receipt = PalwSeatReceiptV4 {
            receipt: PalwSeatReceiptV2 { claim: duty.claim_id, verdict, seat_bond: bond_key, signed_daa, signature },
            shard: place.shard,
            segments: place.segments,
        };
        let bytes = borsh::to_vec(&receipt).expect("a V4 receipt serializes");
        info!(
            "[{PALW_PANEL}] filed a {} V4 receipt for claim {} shard {} (segments {:#x})",
            super::verdict_name(&verdict),
            duty.claim_id,
            place.shard,
            place.segments.0
        );
        self.config.telemetry.panel_receipt(duty.class_id, super::verdict_name(&verdict));
        books.insert_own(receipt, duty.seat_index, std::time::Instant::now());
        self.flow_context.broadcast_palw_seat_receipt(bytes).await;
    }

    /// **Re-send this seat's own V4 receipts while their duties stand**, re-signed (a new signature over the same message: new bytes
    /// every node admits and relays), on the V2/V3 receipts' backoff and per-tick budget.
    pub(super) async fn tir_shard_resend_v1(
        &self,
        bond_key: PalwBondKeyV2,
        network_domain: Hash64,
        shard_duties: &[PalwSeatDutyV2],
        books: &mut PalwTirShardBooksV1,
    ) {
        if books.filed.is_empty() {
            return;
        }
        let standing: HashSet<(Hash64, u16, u8)> =
            shard_duties.iter().filter_map(|d| d.tir_shard.map(|p| (d.claim_id, p.shard, d.seat_index))).collect();
        let now = std::time::Instant::now();
        let Some(kp) = self.keypair.as_ref() else { return };
        for key in books.own_due(&standing, now, crate::palw_receipt_pool::OWN_RECEIPT_REBROADCASTS_PER_TICK) {
            let Some(own) = books.filed.get_mut(&key) else { continue };
            let message = palw_receipt_message_v4(network_domain, key.0, own.verdict, own.signed_daa, own.shard, own.segments);
            let Some(signature) = Self::sign_hedged(&kp.signing_key, message.as_byte_slice(), PALW_RECEIPT_V4_MLDSA87_CONTEXT) else {
                continue;
            };
            let receipt = PalwSeatReceiptV4 {
                receipt: PalwSeatReceiptV2 { claim: key.0, verdict: own.verdict, seat_bond: bond_key, signed_daa: own.signed_daa, signature },
                shard: own.shard,
                segments: own.segments,
            };
            own.schedule.sent(now);
            let bytes = borsh::to_vec(&receipt).expect("a V4 receipt serializes");
            self.flow_context.broadcast_palw_seat_receipt(bytes).await;
        }
        // A duty that is gone ends its re-sends.
        books.filed.retain(|k, _| standing.contains(k));
    }

    /// **Hand the accusations the cells found, and the run demands, to the court's carrier path** (the one-move pass's
    /// `file_tir_one_move_v1`; the demand's rehearsal on the tip).
    pub(super) fn tir_shard_file_findings_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        books: &mut PalwTirShardBooksV1,
        one_move: &mut super::tir_court::PalwTirOneMoveBooksV1<'_>,
    ) {
        for (message, object, due) in std::mem::take(&mut books.demands) {
            let key = super::tir_court::palw_tir_demand_queue_key_v1(message);
            match self.file_tir_demand_v1(session, &object, key, due, one_move) {
                super::tir_court::PalwTirDemandFiledV1::Filed => {}
                super::tir_court::PalwTirDemandFiledV1::Wait(why) => {
                    info!("[{PALW_PANEL}] a run demand waits: {why}");
                    books.demands.push((message, object, due));
                }
                super::tir_court::PalwTirDemandFiledV1::Refused(why) => warn!("[{PALW_PANEL}] a run demand is refused by the chain: {why}"),
            }
        }
        for finding in std::mem::take(&mut books.findings) {
            if one_move.accused.contains(&finding.target.claim_id) {
                continue;
            }
            one_move.accused.insert(finding.target.claim_id);
            self.file_tir_one_move_v1(&finding.target, finding.due, finding.label, finding.leaf, finding.row, finding.accusation, one_move);
        }
    }

    /// **The objects this node carries for layer-sharded panels, built now**: the registrant's plan declaration
    /// (`--palw-tir-shard-declare`, once per class, signed over `palw_tir_shard_plan_message_v1`) and, for each class with a plan this
    /// node holds the artifact of, each held shard's possession proof for the current span (the draw over the shard's own inventory
    /// rows, `palw_tir_shard_readiness_leaves_v1`). Throttled by [`PALW_TIR_SHARD_CARRY_REPLAN_DAA_V1`]; nothing in shadow mode.
    pub(super) async fn tir_shard_objects_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        bond_key: PalwBondKeyV2,
        network_domain: Hash64,
        current_daa: u64,
        books: &mut PalwTirShardBooksV1,
    ) -> Vec<(String, PalwConsensusObjectV2)> {
        let mut out = Vec::new();
        if self.config.tir_shard_shadow || self.keypair.is_none() {
            return out;
        }
        let plans = session.palw_tir_shard_plans_v1();
        // ---- the registrant's declaration, once ----
        if let Some((class_id, s_l, s_p)) = self.config.tir_shard_declare
            && !plans.iter().any(|(c, _)| *c == class_id)
            && books.carried_plan.get(&class_id).is_none_or(|at| current_daa >= at.saturating_add(PALW_TIR_SHARD_CARRY_REPLAN_DAA_V1))
        {
            let message = palw_tir_shard_plan_message_v1(network_domain, &class_id, s_l, s_p);
            if let Some(signature) = self.sign(message.as_byte_slice(), PALW_TIR_SHARD_PLAN_MLDSA87_CONTEXT) {
                books.carried_plan.insert(class_id, current_daa);
                out.push((
                    format!("the layer-shard plan of class {class_id} ({s_l} shards x {s_p} segments)"),
                    PalwConsensusObjectV2::TirShardPlanDeclared { class_id, s_l, s_p, signature },
                ));
            }
        }
        // ---- the held shards' possession proofs ----
        if plans.is_empty() || !self.flow_context.is_nearly_synced(session).await {
            return out;
        }
        let Some(read) = session.palw_model_registry_v1() else { return out };
        if !read.active || read.span_daa == 0 {
            return out;
        }
        let span_now = current_daa / read.span_daa;
        for (class_id, plan) in plans {
            let Some(class) = read.classes.iter().find(|c| c.class_id == class_id) else { continue };
            for shard in 0..plan.s_l {
                if !self.config.tir_shard_hold.is_empty() && !self.config.tir_shard_hold.contains(&shard) {
                    continue;
                }
                if books.carried_proof.get(&(class_id, shard)).is_some_and(|(span, at)| {
                    *span == span_now || current_daa < at.saturating_add(PALW_TIR_SHARD_CARRY_REPLAN_DAA_V1)
                }) {
                    continue;
                }
                let ready = palw_tir_shard_ready_class_v1(&class_id, plan.s_l, shard);
                let row = read.readiness.iter().find(|r| r.bond == bond_key && r.class_id == ready).map(|r| r.row);
                if row.is_some_and(|r| r.proved_span >= span_now) {
                    continue;
                }
                let Some(Ok(tir)) = self.backends().resolve_tir_v1(class_id, class.artifact_root) else { continue };
                match tir_shard_readiness_object_v1(&tir, bond_key, network_domain, class_id, plan.s_l, shard, span_now, |m, c| self.sign(m, c)) {
                    Ok(object) => {
                        books.carried_proof.insert((class_id, shard), (span_now, current_daa));
                        out.push((format!("the possession proof of class {class_id} shard {shard} (span {span_now})"), object));
                    }
                    Err(why) => crate::palw_backends::note_throttled_v1(&format!("tir-shard-proof-{class_id}-{shard}"), || {
                        format!("[{PALW_PANEL}] no possession proof for class {class_id} shard {shard}: {why}")
                    }),
                }
            }
        }
        out
    }
}

/// **A shard's possession proof**: the class's artifact opened at the leaves `(class, shard, bond, span)` draws from the shard's own
/// inventory rows, built into one multiproof against the registered root, signed under
/// [`PALW_TIR_SHARD_READINESS_MLDSA87_CONTEXT`]. The leaf budget is the readiness proof's own (one carrier).
#[allow(clippy::too_many_arguments)]
pub(crate) fn tir_shard_readiness_object_v1(
    tir: &TirBackendV1,
    bond: PalwBondKeyV2,
    network_domain: Hash64,
    class_id: Hash64,
    s_l: u16,
    shard: u16,
    span: u64,
    sign: impl Fn(&[u8], &[u8]) -> Option<Vec<u8>>,
) -> Result<PalwConsensusObjectV2, String> {
    use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
    use kaspa_consensus_core::palw_model_registry_v1 as registry;
    let (parts, _) = tir_shard_geometry_v1(tir, s_l)?;
    let layers = parts.get(usize::from(shard)).cloned().ok_or("a shard of the partition")?;
    let ranges = palw_tir_shard_inventory_ranges_v1(&tir.space().program, layers, shard == 0, shard + 1 == s_l);
    let draw = palw_tir_shard_readiness_leaves_v1(
        &class_id,
        s_l,
        shard,
        &bond,
        span,
        &ranges,
        registry::PALW_READINESS_V2_CHUNKS_V1 as usize,
    );
    if draw.is_empty() {
        return Err("the shard has no inventory leaves".into());
    }
    let (_root, leaves, drawn) = tir.artifact_readiness_material(&draw).map_err(|e| format!("the drawn leaves cannot be opened: {e}"))?;
    let mut opened: Vec<(u32, kaspa_consensus_core::palw_artifact::PalwArtifactOperandV1)> = Vec::with_capacity(drawn.len());
    let mut bytes = 0usize;
    for (index, operand) in drawn {
        if bytes >= registry::PALW_READINESS_V2_BUDGET_BYTES_V1 {
            break;
        }
        if operand.bytes.len() > registry::PALW_READINESS_V2_LEAF_MAX_BYTES_V1 {
            return Err(format!("leaf {index} is {} bytes: no leaf that large can ride a proof", operand.bytes.len()));
        }
        bytes += operand.bytes.len();
        opened.push((index, operand));
    }
    registry::palw_readiness_v2_opening_is_the_challenge_v1(&draw, &opened.iter().map(|(i, o)| (*i, o.bytes.len())).collect::<Vec<_>>())?;
    opened.sort_by_key(|(index, _)| *index);
    let proof = kaspa_consensus_core::palw_artifact::palw_artifact_multiproof_v1(&leaves, &opened).ok_or("the opened leaves are not the inventory's")?;
    kaspa_consensus_core::palw_artifact::verify_artifact_multiproof_v1(&proof, tir.artifact_root()).map_err(|e| format!("{e}"))?;
    let opened_hashes: Vec<(u32, Hash64)> =
        proof.opened.iter().map(|(index, operand)| (*index, kaspa_consensus_core::palw_artifact::artifact_leaf_v1(operand))).collect();
    let message = palw_tir_shard_readiness_message_v1(network_domain, &bond, &class_id, s_l, shard, span, &opened_hashes);
    let signature = sign(message.as_byte_slice(), PALW_TIR_SHARD_READINESS_MLDSA87_CONTEXT).ok_or("the proof cannot be signed")?;
    Ok(PalwConsensusObjectV2::TirSeatReadinessProved { bond, class_id, shard, span, proof: Box::new(proof), signature })
}
