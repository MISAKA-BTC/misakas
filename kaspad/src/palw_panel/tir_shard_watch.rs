//! **RFC-0006 × G14: the non-seat cell watcher** (`--palw-tir-shard-watch`, agent SHARD, `docs/design/palw/shard-rfc6-10.md` §3).
//!
//! G14 for a sharded claim: one bonded verifier OUTSIDE the seats, with public material only, reaches a conviction or a correctly
//! classified default. Consensus already admits it — `TirShardCourtAccused` takes any Active bond at the floor; a `TirStepRun` unit
//! is a DA demand any Active bond may make inside the non-seat budget — and this module is the verifier:
//!
//! * **targets** — the chain's own list ([`kaspa_consensus_core::palw_tir_shard_watch_v1::palw_tir_shard_watch_duties_v1`]): every
//!   shard of a live claim drawn per shard (lane A's or the permissionless Panel's) that this bond neither seats nor produced, as a
//!   watch duty over the shard's outsider span (the whole shard, every segment);
//! * **the verdict** — the seat's own cell verifier ([`palw_tir_shard_outcome_v1`]) over the claim's public capture, through the watch
//!   duty, which carries no seat and signs nothing ([`palw_tir_shard_watch_finding_v1`]);
//! * **the actions** — a finding is the IR one-move accusation ([`palw_tir_shard_accusation_v1`]) with THIS bond as the accuser,
//!   riding the seats' carrier path (`PalwTirShardBooksV1::findings`); no material is a `TirStepRun` demand on chain (the seat's run
//!   pursuit, on the non-seat budget the fold applies), whose default is `ProducerWithholding`;
//! * **material** — only what is public: the captures the node's pool holds (provider directories, served material) and the
//!   chain's own disclosures (answered runs). The watcher never asks the producer with a seat's signature and never files a receipt.
//!
//! Bounded: at most [`PALW_TIR_SHARD_WATCH_PER_TICK_V1`] shards are verified a tick, each `(claim, shard)` once.

use std::collections::HashMap;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_producer_v2::PalwSeatDutyV2;
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
use kaspa_consensus_core::palw_tir_court_v1::PalwTirCourtRulesV1;
use kaspa_consensus_core::palw_tir_one_move_v1::PalwTirOneMoveAccusationV1;
use kaspa_core::{info, warn};
use misaka_palw_sdk::lineages::tir::TirBackendV1;

use super::super::PALW_PANEL;
use super::super::tir_court::palw_tir_duty_target_v1;
use super::{
    PalwTirShardBooksV1, PalwTirShardFindingV1, PalwTirShardOutcomeV1, palw_tir_shard_accusation_v1, palw_tir_shard_outcome_v1,
};

/// How many watched shards a tick verifies at most (each a run of one shard's cells over the capture).
pub(crate) const PALW_TIR_SHARD_WATCH_PER_TICK_V1: usize = 2;

/// **What the watcher found in one shard.**
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PalwTirShardWatchFindingV1 {
    /// Every cell of the shard is the program's function of the committed inputs: nothing to file (a watcher signs no receipt).
    Valid,
    /// A cell is false and an IR close convicts it: the accusation, with the watcher's bond as accuser.
    Accuse { label: &'static str, leaf: Option<u64>, row: Option<u32>, accusation: Box<PalwTirOneMoveAccusationV1> },
    /// A cell is false and no close convicts it in one move: recorded, nothing filed (a one-move accusation that does not convict
    /// charges its accuser).
    Unconvictable { leaf: Option<u64>, row: Option<u32> },
    /// The cells were not run (a fold, a missing leaf, a refused input): nothing filed.
    Abstain(String),
}

/// **The watcher's verdict on one watched shard and the accusation it builds** — the seat's own verifier and accusation builder,
/// over the watch duty and the claim's public capture, the accuser `watcher`. Pure over its inputs.
#[allow(clippy::too_many_arguments)]
pub(crate) fn palw_tir_shard_watch_finding_v1(
    tir: &TirBackendV1,
    material: &[u8],
    duty: &PalwSeatDutyV2,
    watcher: PalwBondKeyV2,
    court: &kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2,
    rules: &PalwTirCourtRulesV1,
    ladder: u64,
    form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
    device: &mut dyn misaka_palw_sdk::lineages::tir::KernelBackendV1,
) -> Result<PalwTirShardWatchFindingV1, String> {
    if duty.tir_shard.is_none() {
        return Err("a watch duty names a shard".to_string());
    }
    Ok(match palw_tir_shard_outcome_v1(tir, material, duty, device)? {
        PalwTirShardOutcomeV1::Valid { .. } => PalwTirShardWatchFindingV1::Valid,
        PalwTirShardOutcomeV1::Abstain(why) => PalwTirShardWatchFindingV1::Abstain(why),
        PalwTirShardOutcomeV1::Fault { leaf, row, .. } => {
            let target = palw_tir_duty_target_v1(duty);
            match palw_tir_shard_accusation_v1(tir, material, leaf, row, &target, watcher, court, rules, ladder, form)? {
                Some((label, accusation)) => PalwTirShardWatchFindingV1::Accuse { label, leaf, row, accusation: Box::new(accusation) },
                None => PalwTirShardWatchFindingV1::Unconvictable { leaf, row },
            }
        }
    })
}

impl super::super::PalwPanelService {
    /// **The watcher's pass** (after the seat pass, `--palw-tir-shard-watch`): up to [`PALW_TIR_SHARD_WATCH_PER_TICK_V1`] watched
    /// shards this tick, each once. A shard of a class this node does not hold is skipped (a watcher holds the class it watches); a
    /// claim with no public capture is pursued through `TirStepRun` demands (`--palw-tir-shard-demand-runs`), never by a signed
    /// material request.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn tir_shard_watch_pass_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        bond_key: PalwBondKeyV2,
        network_domain: Hash64,
        current_daa: u64,
        watch: &[PalwSeatDutyV2],
        materials: &HashMap<Hash64, Vec<Vec<u8>>>,
        interval_openings: &mut HashMap<(Hash64, u32), Vec<Vec<u8>>>,
        books: &mut PalwTirShardBooksV1,
    ) {
        let mut verified = 0usize;
        for duty in watch {
            if verified >= PALW_TIR_SHARD_WATCH_PER_TICK_V1 {
                break;
            }
            let Some(place) = duty.tir_shard else { continue };
            if books.watched.contains(&(duty.claim_id, place.shard))
                || books.refuted.contains(&duty.claim_id)
                || current_daa > duty.receipt_deadline
            {
                continue;
            }
            let tir = match self.backends().resolve_tir_v1(duty.class_id, duty.artifact_root) {
                Some(Ok(tir)) => std::sync::Arc::new(tir),
                _ => continue,
            };
            let material = materials
                .get(&duty.claim_id)
                .and_then(|pool| {
                    pool.iter().find(|m| {
                        tir.decode_capture(m).is_ok_and(|c| {
                            c.binding.committed_execution_root == duty.execution_root
                                && c.binding.full_logits_trace_root == duty.trace_root
                        })
                    })
                })
                .cloned()
                .or_else(|| books.synthesised.get(&duty.claim_id).cloned());
            let Some(material) = material else {
                // No public capture: the chain's own units, past a quarter of the claim's window (the non-seat budget applies).
                let window = duty.receipt_deadline.saturating_sub(duty.bound_daa);
                if self.config.tir_shard_demand_runs
                    && !books.demanded.contains(&duty.claim_id)
                    && current_daa >= duty.bound_daa.saturating_add(window / 4)
                {
                    self.tir_shard_run_pursuit_v1(
                        session,
                        bond_key,
                        network_domain,
                        current_daa,
                        duty,
                        &tir,
                        interval_openings,
                        books,
                    )
                    .await;
                }
                continue;
            };
            verified += 1;
            let court = self.config.court;
            let network_ladder = kaspa_consensus_core::palw_court_v2::palw_refutation_leaf_cap_v2(
                &court,
                self.consensus_config.params.palw_court_ladder.is_some_and(|f| f.is_active(current_daa)),
            );
            let ladder = self.seat_refutation_ladder_v1(duty.class_id, network_ladder, current_daa);
            let form = self.config.prompt_ids_form;
            let mut rules = tir.court_rules(&court);
            rules.max_step_leaf_count = ladder;
            let (task_tir, task_duty) = (tir.clone(), duty.clone());
            let found = tokio::task::spawn_blocking(move || {
                let mut cpu = misaka_palw_sdk::lineages::tir::CpuKernelBackendV1;
                palw_tir_shard_watch_finding_v1(&task_tir, &material, &task_duty, bond_key, &court, &rules, ladder, form, &mut cpu)
            })
            .await;
            let found = match found {
                Ok(Ok(found)) => found,
                Ok(Err(why)) => {
                    crate::palw_backends::note_throttled_v1(&format!("tir-shard-watch-{}", duty.claim_id), || {
                        format!("[{PALW_PANEL}] watch: claim {} shard {}: the cells do not run ({why})", duty.claim_id, place.shard)
                    });
                    continue;
                }
                Err(_) => continue,
            };
            books.watched.insert((duty.claim_id, place.shard));
            match found {
                PalwTirShardWatchFindingV1::Valid => {
                    books.cells_verified += 1;
                    info!(
                        "[{PALW_PANEL}] watch: claim {} shard {}/{}: every cell verifies (no seat: nothing filed)",
                        duty.claim_id, place.shard, place.s_l
                    );
                }
                PalwTirShardWatchFindingV1::Accuse { label, leaf, row, accusation } => {
                    warn!(
                        "[{PALW_PANEL}] watch: claim {} shard {}/{}: a cell finds the executor's commitment false (leaf {leaf:?}, token row \
                         {row:?}) — accusing as a non-seat bond ({label})",
                        duty.claim_id, place.shard, place.s_l
                    );
                    books.refuted.insert(duty.claim_id);
                    let target = palw_tir_duty_target_v1(duty);
                    let earliest_final =
                        super::super::palw_seat_claim_earliest_final_v1(&self.consensus_config.params, duty.bound_daa);
                    let due = super::super::palw_seat_court_filing_due_v1(duty.receipt_deadline, earliest_final, current_daa);
                    books.findings.push(PalwTirShardFindingV1 { target, due, label, leaf, row, accusation: *accusation });
                }
                PalwTirShardWatchFindingV1::Unconvictable { leaf, row } => warn!(
                    "[{PALW_PANEL}] watch: claim {} shard {}: a cell is false (leaf {leaf:?}, row {row:?}) but no IR close convicts it in one \
                     move; recorded, not filed",
                    duty.claim_id, place.shard
                ),
                PalwTirShardWatchFindingV1::Abstain(why) => {
                    crate::palw_backends::note_throttled_v1(&format!("tir-shard-watch-abstain-{}", duty.claim_id), || {
                        format!("[{PALW_PANEL}] watch: claim {} shard {}: the cells are not run ({why})", duty.claim_id, place.shard)
                    });
                }
            }
        }
    }
}
