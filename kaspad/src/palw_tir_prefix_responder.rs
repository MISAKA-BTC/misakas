//! Literal paid TIR prefix-state duties (behind `palw_staged_tir_route_v1`). The duty page is the
//! shared streamed-source index; a key whose bundle's active stage is a TIR prefix court is
//! answered here, one authenticated boundary leaf per move, from this node's retained TIR
//! capture through the backend's `disclose_tir_step_leaf_v1` (RFC-0002's TIR executor). Only the
//! accepted slot cursor acknowledges a leaf.
use super::*;
use kaspa_consensus_core::{
    palw_constraint_stream_carriage_v1::{
        PALW_CONSTRAINT_STREAM_CONTEXT_V1, PalwConstraintStreamActionV1, PalwConstraintStreamMoveV1,
    },
    palw_constraint_stream_court_v1::PalwConstraintStreamPhaseV1,
    palw_tir_prefix_owned_v1::PalwTirPrefixLeafAnswerV1,
};

/// Its own lane round, below the Float32 responder's (`u32::MAX - 22`).
const ROUND: u32 = u32::MAX - 23;
#[derive(Default)]
pub(super) struct TirPrefixBookV1 {
    after: Option<(u64, Hash64, PalwBondKeyV2)>,
}

impl PalwPanelService {
    pub(super) async fn public_tir_prefix_answer_tick_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        slot: super::PalwAnswerSlotV1<'_>,
        book: &mut TirPrefixBookV1,
        pending: &mut Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
        due: &mut HashMap<(Hash64, u32, bool), u64>,
        moved: &mut HashMap<(Hash64, u32, bool), u64>,
    ) -> Result<(), String> {
        if self.consensus_config.params.palw_staged_public_court_v1.is_none()
            || self.consensus_config.params.palw_staged_tir_route_v1_fence().is_none()
        {
            return Ok(());
        }
        let signer = slot.slot;
        let lane_round = ROUND - slot.round_offset;
        let (keys, next) = session.palw_staged_flat_duties_v1(signer, book.after)?;
        if keys.len() > 64 {
            return Err("TIR prefix duties exceed native bounded page".into());
        }
        book.after = next;
        // A queued leaf whose slot cursor moved on (or whose court closed) is stale.
        pending.retain(|(_, round, responder, object)| {
            if (*round, *responder) != (lane_round, true) {
                return true;
            }
            let PalwConsensusObjectV2::PublicCourtStreamMoveV1 { movement } = object else { return false };
            let PalwConstraintStreamActionV1::NativeTirPrefixAnswer(answer) = &movement.action else { return false };
            if answer.answerer != signer {
                return true;
            }
            match session.palw_staged_tir_prefix_v1((movement.claim, movement.reserver)) {
                Err(_) => true,
                Ok(None) => false,
                Ok(Some(p)) => {
                    p.row.seed_id() == answer.seed_id
                        && p.row.has_response_duty_v1(&signer)
                        && p.now_daa < p.row.respond_until_daa
                        && p.row.slot_ordinal_v1(&signer) == Some(answer.ordinal)
                }
            }
        });
        for (claim, owner) in keys {
            if slot.only.is_some_and(|only| !only.contains(&claim)) {
                continue;
            }
            if pending.iter().filter(|(_, r, b, _)| (*r, *b) == (lane_round, true)).count() >= 64 {
                break;
            }
            let mut attempt = None;
            let result = async {
                // A Flat or Float32 bundle on the shared page is answered by its own responder.
                let Some(progress) = session.palw_staged_tir_prefix_v1((claim, owner))? else { return Ok(()) };
                let row = &progress.row;
                if !row.has_response_duty_v1(&signer) || progress.now_daa >= row.respond_until_daa {
                    return Ok(());
                }
                let ordinal = row.slot_ordinal_v1(&signer).ok_or("TIR prefix signer has no literal slot")?;
                let key = (row.seed_id(), lane_round, true);
                if pending.iter().any(|(a, b, c, _)| (*a, *b, *c) == key)
                    || moved.get(&key).is_some_and(|at| progress.now_daa < at.saturating_add(COURT_MOVE_REPLAN_DAA))
                {
                    return Ok(());
                }
                attempt = Some((key, progress.now_daa));
                let class = progress.header.job_context.shape_profile_id;
                let maximum = self.class_step_ladder(class);
                let leaf_index = session
                    .palw_staged_tir_prefix_leaf_index_v1((claim, owner), ordinal, maximum)?
                    .ok_or("TIR prefix slot names no boundary leaf")?;
                let facts = session.palw_producer_facts_v2(class, None).ok_or("TIR prefix class artifact unavailable")?;
                let backend: Arc<dyn kaspa_consensus_core::palw_backend::PalwExecutionBackendV1> =
                    if row.producer == signer && slot.only.is_none() {
                        self.executor_backend_v1(session, class, facts.artifact_root)?
                    } else {
                        Arc::from(self.resolve_backend(session, class, facts.artifact_root)?)
                    };
                let paths = self.fp_retained_payload_paths(&claim);
                let header = progress.header.clone();
                let leaf = tokio::task::spawn_blocking(move || {
                    for path in paths {
                        let Ok(material) = std::fs::read(&path) else { continue };
                        if let Ok(leaf) = backend.disclose_tir_step_leaf_v1(&material, &header, leaf_index, maximum) {
                            return Ok(leaf);
                        }
                    }
                    Err::<_, String>("TIR prefix responder has no retained TIR capture for this claim".into())
                })
                .await
                .map_err(|e| format!("TIR prefix responder worker failed: {e}"))??;
                let answer = PalwTirPrefixLeafAnswerV1 { answerer: signer, seed_id: row.seed_id(), ordinal, values_le: leaf.0, opening: leaf.1 };
                answer.check_wire_v1()?;
                let mut movement = PalwConstraintStreamMoveV1 {
                    version: 1,
                    claim,
                    reserver: owner,
                    reserved_daa: row.reserved_daa,
                    seed_id: row.seed_id(),
                    phase: PalwConstraintStreamPhaseV1::PrefixCourt,
                    action: PalwConstraintStreamActionV1::NativeTirPrefixAnswer(answer),
                    signature: Vec::new(),
                };
                movement.signature = self
                    .sign(&movement.signing_message(), PALW_CONSTRAINT_STREAM_CONTEXT_V1)
                    .ok_or("TIR prefix custodian signing key unavailable")?;
                movement.check()?;
                let deadline = row.respond_until_daa;
                pending.push((key.0, key.1, key.2, PalwConsensusObjectV2::PublicCourtStreamMoveV1 { movement: Box::new(movement) }));
                due.insert(key, deadline.saturating_sub(1));
                moved.insert(key, progress.now_daa);
                Ok::<(), String>(())
            }
            .await;
            if let Err(why) = result {
                if let Some((key, clock)) = attempt {
                    moved.insert(key, clock);
                }
                debug!("[{PALW_PANEL}] claim {claim}: TIR prefix-state response remains pending: {why}");
            }
        }
        Ok(())
    }
}
