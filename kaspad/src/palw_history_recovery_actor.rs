//! Ordinary paid history recovery. Native accepted state owns all clocks and turns;
//! peer nonresponse merely schedules a demand and never proves a semantic fault.
use kaspa_consensus_core::{
    Hash64,
    palw_constraint_stream_carriage_v1::{PalwConstraintStreamActionV1, PalwConstraintStreamMoveV1},
    palw_constraint_stream_court_v1::PalwConstraintStreamPhaseV1,
    palw_history_custody_v1::{PALW_HISTORY_CUSTODY_PART_BYTES_V1, PalwHistoryRecoveryActionV1},
    palw_prefix_history_v1::{PALW_PREFIX_HISTORY_MAX_RECORD_BYTES_V1, PalwPrefixHistoryRecordV1, PalwPrefixHistoryRequestV1},
    palw_staged_court_state_v1::PalwStagedHistoryRecoveryProgressV1,
    palw_state_v2::PalwBondKeyV2,
};

/// Distinct from the Flat responder (`u32::MAX - 17`): both queues retain entries by round.
pub(super) const HISTORY_ROUND_V1: u32 = u32::MAX - 18;
const HISTORY_CARRIER_RAM_V1: u64 = PALW_PREFIX_HISTORY_MAX_RECORD_BYTES_V1 as u64 * 8 + 65_536;
pub(super) type HistoryDutyCursorV1 = (u64, Hash64, PalwBondKeyV2, Hash64);

pub(super) enum HistoryOwnerDecisionV1 {
    Move {
        movement: PalwConstraintStreamMoveV1,
        until_daa: u64,
    },
    Wait,
    Recovered,
    /// Native has already charged the exact once-only DA default. Not compute guilt.
    Default,
}

fn movement_v1(
    progress: &PalwStagedHistoryRecoveryProgressV1,
    request: &PalwPrefixHistoryRequestV1,
    demand_opened_daa: u64,
    action: PalwHistoryRecoveryActionV1,
) -> PalwConstraintStreamMoveV1 {
    PalwConstraintStreamMoveV1 {
        version: 1,
        claim: progress.initial.claim,
        reserver: progress.initial.owner,
        reserved_daa: progress.initial.reserved_daa,
        seed_id: progress.bundle_id,
        phase: PalwConstraintStreamPhaseV1::PrefixCourt,
        action: PalwConstraintStreamActionV1::HistoryRecovery {
            stage_seed: progress.initial.seed_id,
            request_head: request.head,
            sequence: request.sequence,
            demand_opened_daa,
            action,
        },
        signature: Vec::new(),
    }
}

/// Open is the first native Demand; it pins the original retained stage/head only
/// once. Restart resumes that accepted session, never reopens its paid window.
pub(super) fn owner_request_v1(progress: &PalwStagedHistoryRecoveryProgressV1) -> Result<HistoryOwnerDecisionV1, String> {
    progress.initial.check_v1()?;
    if progress.now_daa < progress.terms.accepted_daa || progress.now_daa >= progress.terms.collect_until_daa {
        return Ok(HistoryOwnerDecisionV1::Wait);
    }
    if let Some(row) = &progress.session {
        if row.initial != progress.initial {
            return Err("history owner snapshot substitutes its native pinned initial request".into());
        }
        if row.default.is_some() {
            return Ok(HistoryOwnerDecisionV1::Default);
        }
        if row.complete_v1() {
            return Ok(HistoryOwnerDecisionV1::Recovered);
        }
        if let Some(demand) = &row.demand {
            return Ok(if progress.now_daa >= demand.answer_until_daa {
                HistoryOwnerDecisionV1::Move {
                    movement: movement_v1(progress, &demand.request, demand.opened_daa, PalwHistoryRecoveryActionV1::Collect),
                    until_daa: row.collect_until_daa,
                }
            } else {
                HistoryOwnerDecisionV1::Wait
            });
        }
        let request = row.remaining_request_v1().ok_or("history recovery has a noncanonical remaining cursor")?;
        if progress.now_daa.checked_add(row.response_span_daa).is_none_or(|until| until > row.recovery_until_daa) {
            return Ok(HistoryOwnerDecisionV1::Wait);
        }
        return Ok(HistoryOwnerDecisionV1::Move {
            movement: movement_v1(progress, &request, 0, PalwHistoryRecoveryActionV1::Demand),
            until_daa: row.recovery_until_daa.saturating_sub(row.response_span_daa).saturating_add(1),
        });
    }
    if progress.now_daa.checked_add(progress.response_span_daa).is_none_or(|until| until > progress.terms.stream_until_daa) {
        return Ok(HistoryOwnerDecisionV1::Wait);
    }
    Ok(HistoryOwnerDecisionV1::Move {
        movement: movement_v1(progress, &progress.initial, 0, PalwHistoryRecoveryActionV1::Demand),
        until_daa: progress.terms.stream_until_daa.saturating_sub(progress.response_span_daa).saturating_add(1),
    })
}

pub(super) fn response_v1(
    progress: &PalwStagedHistoryRecoveryProgressV1,
    signer: PalwBondKeyV2,
    raw: &[u8],
) -> Result<Option<(PalwConstraintStreamMoveV1, u64)>, String> {
    let Some(row) = &progress.session else { return Ok(None) };
    let Some(demand) = &row.demand else { return Ok(None) };
    if row.default.is_some()
        || progress.now_daa < demand.opened_daa
        || progress.now_daa >= demand.answer_until_daa
        || (!row.custodians.contains(&signer) && signer != row.initial.owner)
    {
        return Ok(None);
    }
    // Authenticate bounded raw bytes BEFORE nested Borsh allocation or semantic use.
    PalwPrefixHistoryRecordV1::decode_authenticated_request_v1(raw, &demand.request)?;
    let action = match demand.buffers.get(&signer) {
        None => PalwHistoryRecoveryActionV1::Begin { answerer: signer, total_bytes: raw.len() as u32 },
        Some(buffer) => {
            if buffer.total_bytes as usize != raw.len() || !raw.starts_with(&buffer.bytes) {
                return Err("history accepted answer buffer differs from exact rooted original record".into());
            }
            let offset = buffer.bytes.len();
            if offset == raw.len() {
                PalwHistoryRecoveryActionV1::Finish { answerer: signer }
            } else {
                PalwHistoryRecoveryActionV1::Part {
                    answerer: signer,
                    offset: offset as u32,
                    bytes: raw[offset..raw.len().min(offset + PALW_HISTORY_CUSTODY_PART_BYTES_V1)].to_vec(),
                }
            }
        }
    };
    Ok(Some((movement_v1(progress, &demand.request, demand.opened_daa, action), demand.answer_until_daa)))
}

/// This is only a LOCAL queue identity. The signed native carrier preserves the
/// actual claim/owner/seed, so simultaneous owners cannot overwrite each other.
pub(super) fn queue_scope_v1(progress: &PalwStagedHistoryRecoveryProgressV1, signer: PalwBondKeyV2) -> Hash64 {
    let raw = borsh::to_vec(&(
        progress.initial.claim,
        progress.initial.owner,
        progress.initial.reserved_daa,
        progress.bundle_id,
        progress.initial.seed_id,
        signer,
    ))
    .expect("fixed history queue scope serializes");
    Hash64::from_bytes(
        blake2b_simd::Params::new()
            .hash_length(64)
            .key(b"misaka-palw/node-history-queue/v1")
            .hash(&raw)
            .as_bytes()
            .try_into()
            .expect("64"),
    )
}

fn queued_response_current_v1(
    progress: &PalwStagedHistoryRecoveryProgressV1,
    signer: PalwBondKeyV2,
    movement: &PalwConstraintStreamMoveV1,
) -> bool {
    let Some(row) = &progress.session else { return false };
    let Some(demand) = &row.demand else { return false };
    let PalwConstraintStreamActionV1::HistoryRecovery { stage_seed, request_head, sequence, demand_opened_daa, action } =
        &movement.action
    else {
        return false;
    };
    if movement.claim != progress.initial.claim
        || movement.reserver != progress.initial.owner
        || movement.reserved_daa != progress.initial.reserved_daa
        || movement.seed_id != progress.bundle_id
        || *stage_seed != progress.initial.seed_id
        || (*request_head, *sequence, *demand_opened_daa) != (demand.request.head, demand.request.sequence, demand.opened_daa)
        || action.answerer_v1() != Some(signer)
        || progress.now_daa >= demand.answer_until_daa
        || row.default.is_some()
        || !row.custodians.contains(&signer)
    {
        return false;
    }
    match action {
        PalwHistoryRecoveryActionV1::Begin { .. } => !demand.buffers.contains_key(&signer),
        PalwHistoryRecoveryActionV1::Part { offset, .. } => {
            demand.buffers.get(&signer).is_some_and(|buffer| buffer.bytes.len() == *offset as usize)
        }
        PalwHistoryRecoveryActionV1::Finish { .. } => {
            demand.buffers.get(&signer).is_some_and(|buffer| buffer.bytes.len() == buffer.total_bytes as usize)
        }
        _ => false,
    }
}

#[derive(Default)]
pub(super) struct HistoryResponderBookV1 {
    pub(super) cursor: Option<HistoryDutyCursorV1>,
    held: std::collections::HashMap<Hash64, crate::palw_memory_ledger::PalwMemoryReservationV1>,
}

impl super::PalwPanelService {
    pub(super) async fn history_recovery_answer_tick_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        slot: super::PalwAnswerSlotV1<'_>,
        book: &mut HistoryResponderBookV1,
        pending: &mut Vec<(Hash64, u32, bool, kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2)>,
        due: &mut std::collections::HashMap<(Hash64, u32, bool), u64>,
        moved: &mut std::collections::HashMap<(Hash64, u32, bool), u64>,
    ) -> Result<(), String> {
        use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;
        let now = session.palw_v2_daa_score().ok_or("history responder has no native DAA")?;
        if !self.consensus_config.params.palw_staged_public_court_v1_active_at(now) {
            return Ok(());
        }
        let metadata = crate::palw_memory_ledger::host_ledger_v1()
            .reserve(
                crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
                    role: "history-duty-page",
                    class_id: Hash64::default(),
                    job: Hash64::default(),
                },
                1_048_576,
            )
            .map_err(|e| e.to_string())?;
        let signer = slot.slot;
        let lane_round = HISTORY_ROUND_V1 - slot.round_offset;
        let page = session.palw_history_response_duties_v1(signer, book.cursor)?;
        book.cursor = page.next;
        pending.retain(|(_, round, responder, object)| {
            if (*round, *responder) != (lane_round, true) {
                return true;
            }
            let PalwConsensusObjectV2::PublicCourtStreamMoveV1 { movement } = object else { return false };
            let PalwConstraintStreamActionV1::HistoryRecovery { stage_seed, action, .. } = &movement.action else { return false };
            if action.answerer_v1() != Some(signer) {
                return true;
            }
            match session.palw_staged_history_recovery_v1((movement.claim, movement.reserver), *stage_seed) {
                Err(_) => true,
                Ok(None) => false,
                Ok(Some(progress)) => queued_response_current_v1(&progress, signer, movement),
            }
        });
        book.held.retain(|scope, _| pending.iter().any(|(a, b, c, _)| (*a, *b, *c) == (*scope, lane_round, true)));
        for (claim, owner, stage) in page.keys {
            if slot.only.is_some_and(|only| !only.contains(&claim)) {
                continue;
            }
            if book.held.len() >= 64 {
                break;
            }
            let result = (|| -> Result<_, String> {
                let Some(progress) = session.palw_staged_history_recovery_v1((claim, owner), stage)? else { return Ok(None) };
                if progress.snapshot_block != page.snapshot_block || progress.now_daa != page.now_daa {
                    return Ok(None);
                }
                let scope = queue_scope_v1(&progress, signer);
                let key = (scope, lane_round, true);
                if pending.iter().any(|(a, b, c, _)| (*a, *b, *c) == key)
                    || moved.get(&key).is_some_and(|at| progress.now_daa >= *at && progress.now_daa < at.saturating_add(3))
                {
                    return Ok(None);
                }
                let Some(row) = &progress.session else { return Ok(None) };
                let Some(demand) = &row.demand else { return Ok(None) };
                if !row.custodians.contains(&signer) || progress.now_daa >= demand.answer_until_daa {
                    return Ok(None);
                }
                let guard = crate::palw_memory_ledger::host_ledger_v1()
                    .reserve(
                        crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
                            role: "history-custody-carrier",
                            class_id: Hash64::default(),
                            job: scope,
                        },
                        HISTORY_CARRIER_RAM_V1,
                    )
                    .map_err(|e| e.to_string())?;
                // No backend, reexecution, private snapshot or membership-based liability.
                // The native exporter returns the original accepted carrier, not recovery.
                let record = match session.palw_prefix_history_record_current_v1(demand.request.clone())? {
                    Some(record) => record,
                    // A pruned-synced or restored custodian may lack its own archive copy. The
                    // bounded public peer lane can still supply the exact rooted original bytes;
                    // they are authenticated against the accepted request before any answer.
                    None => match self.flow_context.palw_prefix_history().poll_v1(&demand.request)? {
                        Some(record) => {
                            record.authenticates_request_v1(&demand.request)?;
                            record
                        }
                        None => return Ok(None),
                    },
                };
                let raw = borsh::to_vec(&record).map_err(|e| e.to_string())?;
                let Some((mut movement, until)) = response_v1(&progress, signer, &raw)? else { return Ok(None) };
                let kp = self.keypair.as_ref().ok_or("history custodian signing key unavailable")?;
                movement.signature = libcrux_ml_dsa::ml_dsa_87::sign(
                    &kp.signing_key,
                    &movement.signing_message(),
                    kaspa_consensus_core::palw_constraint_stream_carriage_v1::PALW_CONSTRAINT_STREAM_CONTEXT_V1,
                    [0; 32],
                )
                .map_err(|e| format!("history carrier signature: {e:?}"))?
                .as_ref()
                .to_vec();
                movement.check()?;
                let object = PalwConsensusObjectV2::PublicCourtStreamMoveV1 { movement: Box::new(movement) };
                if borsh::object_length(&object).map_err(|e| e.to_string())? > 90_000 {
                    return Err("history carrier exceeds original paid ordinary lane".into());
                }
                Ok(Some((key, object, until, guard)))
            })();
            match result {
                Ok(Some((key, object, until, guard))) => {
                    due.insert(key, until.saturating_sub(1));
                    pending.push((key.0, key.1, key.2, object));
                    book.held.insert(key.0, guard);
                }
                Ok(None) => {}
                Err(why) => {
                    kaspa_core::debug!("history responder claim {claim}: {why}");
                }
            }
        }
        drop(metadata);
        Ok(())
    }
}

/// Distinct from every other court-queue round: a seat's own signed history custody grant.
pub(super) const HISTORY_GRANT_ROUND_V1: u32 = u32::MAX - 21;
const HISTORY_GRANTS_QUEUED_V1: usize = 16;

/// The exact grant and its carrier for one already credited own Valid receipt. Pure: the
/// caller signs both messages with the seat's key and checks the carrier before queueing.
pub(super) fn grant_movement_v1(
    signer: PalwBondKeyV2,
    receipt: &kaspa_consensus_core::palw_panel_v2::PalwSeatReceiptV2,
    duty: &kaspa_consensus_core::palw_staged_court_state_v1::PalwHistoryGrantDutyV1,
) -> Result<(kaspa_consensus_core::palw_history_custody_v1::PalwHistoryCustodyGrantV1, PalwConstraintStreamMoveV1), String> {
    use kaspa_consensus_core::palw_history_custody_v1::PalwHistoryCustodyGrantV1;
    if receipt.claim != duty.claim
        || receipt.seat_bond != signer
        || receipt.verdict != kaspa_consensus_core::palw_panel_v2::PalwReceiptVerdictV2::Valid
    {
        return Err("history grant must bind this seat's own signed Valid receipt for the duty's claim".into());
    }
    let grant = PalwHistoryCustodyGrantV1 {
        version: 1,
        claim: duty.claim,
        seat: signer,
        signed_daa: receipt.signed_daa,
        pledge_id: duty.pledge_id,
        valid_receipt_digest: PalwHistoryCustodyGrantV1::receipt_digest_v1(receipt),
        signature: Vec::new(),
    };
    let movement = PalwConstraintStreamMoveV1 {
        version: 1,
        claim: duty.claim,
        reserver: signer,
        reserved_daa: duty.accepted_daa,
        seed_id: duty.pledge_id,
        phase: PalwConstraintStreamPhaseV1::PrefixCourt,
        action: PalwConstraintStreamActionV1::HistoryCustodyGrant { grant: grant.clone(), receipt: receipt.clone() },
        signature: Vec::new(),
    };
    Ok((grant, movement))
}

impl super::PalwPanelService {
    /// A Valid seat of a required-family claim opts into the producer's public history custody
    /// with its own signed grant, once native state has credited that exact receipt. Afterwards
    /// it answers history demands from its native archive (or the peer lane) like the producer,
    /// and a native history default may charge it as a literal custodian.
    pub(super) async fn history_custody_grant_tick_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        signer: PalwBondKeyV2,
        own_receipts: &std::collections::HashMap<Hash64, (kaspa_consensus_core::palw_panel_v2::PalwSeatReceiptV2, u64)>,
        pending: &mut Vec<(Hash64, u32, bool, kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2)>,
        due: &mut std::collections::HashMap<(Hash64, u32, bool), u64>,
        moved: &mut std::collections::HashMap<(Hash64, u32, bool), u64>,
    ) -> Result<(), String> {
        use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;
        let Some(now) = session.palw_v2_daa_score() else { return Ok(()) };
        if !self.consensus_config.params.palw_staged_public_court_v1_active_at(now) {
            return Ok(());
        }
        // A queued grant whose duty is gone (credited, expired or another branch) is dropped.
        pending.retain(|(claim, round, responder, object)| {
            if (*round, *responder) != (HISTORY_GRANT_ROUND_V1, true) {
                return true;
            }
            let PalwConsensusObjectV2::PublicCourtStreamMoveV1 { movement } = object else { return false };
            let PalwConstraintStreamActionV1::HistoryCustodyGrant { receipt, .. } = &movement.action else { return false };
            match session.palw_staged_history_grant_duty_v1(*claim, signer, receipt.clone()) {
                Err(_) => true,
                Ok(duty) => duty.is_some(),
            }
        });
        let mut queued = pending.iter().filter(|(_, round, responder, _)| (*round, *responder) == (HISTORY_GRANT_ROUND_V1, true)).count();
        for (claim, (receipt, _)) in own_receipts {
            if queued >= HISTORY_GRANTS_QUEUED_V1 {
                break;
            }
            if receipt.verdict != kaspa_consensus_core::palw_panel_v2::PalwReceiptVerdictV2::Valid || receipt.seat_bond != signer {
                continue;
            }
            let key = (*claim, HISTORY_GRANT_ROUND_V1, true);
            if pending.iter().any(|(a, b, c, _)| (*a, *b, *c) == key)
                || moved.get(&key).is_some_and(|at| now < at.saturating_add(super::COURT_MOVE_REPLAN_DAA))
            {
                continue;
            }
            let duty = match session.palw_staged_history_grant_duty_v1(*claim, signer, receipt.clone()) {
                Ok(Some(duty)) => duty,
                Ok(None) => continue,
                Err(why) => {
                    kaspa_core::debug!("history grant claim {claim}: {why}");
                    continue;
                }
            };
            let (mut grant, mut movement) = grant_movement_v1(signer, receipt, &duty)?;
            grant.signature = self
                .sign(
                    grant.signing_message_v1().as_byte_slice(),
                    kaspa_consensus_core::palw_history_custody_v1::PALW_HISTORY_GRANT_CONTEXT_V1,
                )
                .ok_or("history grant signing key unavailable")?;
            movement.action = PalwConstraintStreamActionV1::HistoryCustodyGrant { grant, receipt: receipt.clone() };
            movement.signature = self
                .sign(&movement.signing_message(), kaspa_consensus_core::palw_constraint_stream_carriage_v1::PALW_CONSTRAINT_STREAM_CONTEXT_V1)
                .ok_or("history grant carrier signing key unavailable")?;
            movement.check()?;
            pending.push((key.0, key.1, key.2, PalwConsensusObjectV2::PublicCourtStreamMoveV1 { movement: Box::new(movement) }));
            due.insert(key, duty.material_until_daa.saturating_sub(1));
            moved.insert(key, now);
            queued += 1;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::{
        BlockHash,
        palw_history_custody_v1::{PalwHistoryRecoveryEffectV1, PalwHistoryRecoveryV1, history_recovery_resource_plan_v1},
        palw_prefix_family_court_v1::PalwPrefixCourtActionV1,
        palw_rendering_resources_v1::{PalwRenderingClaimTermsV1, PalwStagedPublicCourtConfigV1},
        tx::{TransactionId, TransactionOutpoint},
    };
    fn bond(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_u64_word(n), 0))
    }
    fn record(previous: Hash64, sequence: u64) -> PalwPrefixHistoryRecordV1 {
        PalwPrefixHistoryRecordV1::new(
            previous,
            sequence,
            BlockHash::from_u64_word(sequence),
            100 + sequence,
            500,
            PalwConstraintStreamMoveV1 {
                version: 1,
                claim: Hash64::from_u64_word(1),
                reserver: bond(2),
                reserved_daa: 90,
                seed_id: Hash64::from_u64_word(3),
                phase: PalwConstraintStreamPhaseV1::PrefixCourt,
                action: PalwConstraintStreamActionV1::Prefix(PalwPrefixCourtActionV1::FinishTile),
                signature: vec![7; kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
            },
        )
        .unwrap()
    }
    fn progress(record: &PalwPrefixHistoryRecordV1) -> PalwStagedHistoryRecoveryProgressV1 {
        PalwStagedHistoryRecoveryProgressV1 {
            snapshot_block: BlockHash::from_u64_word(6),
            now_daa: 110,
            bundle_id: Hash64::from_u64_word(4),
            terms: PalwRenderingClaimTermsV1 {
                accepted_daa: 90,
                open_until_daa: 120,
                stream_until_daa: 400,
                collect_until_daa: 450,
                material_until_daa: 500,
                bond_exit_after_daa: 501,
                plan: history_recovery_resource_plan_v1(2, 1, PalwStagedPublicCourtConfigV1::network_v1().prices).unwrap(),
            },
            initial: PalwPrefixHistoryRequestV1 {
                claim: record.movement.claim,
                owner: record.movement.reserver,
                reserved_daa: record.movement.reserved_daa,
                seed_id: record.movement.seed_id,
                head: record.head_v1().unwrap(),
                sequence: record.sequence,
                material_until_daa: record.material_until_daa,
            },
            response_span_daa: 20,
            maximum_custodians: 1,
            session: None,
        }
    }
    fn apply(row: &mut PalwHistoryRecoveryV1, movement: &PalwConstraintStreamMoveV1, now: u64) -> PalwHistoryRecoveryEffectV1 {
        movement.check_history_scope_v1(row).unwrap();
        let PalwConstraintStreamActionV1::HistoryRecovery { action, .. } = &movement.action else { panic!("typed history move") };
        row.apply_v1(action, now).unwrap()
    }
    #[test]
    fn cold_actor_resumes_native_offsets_and_old_carrier_cannot_poison_next_record() {
        let first = record(Hash64::default(), 1);
        let second = record(first.head_v1().unwrap(), 2);
        let mut progress = progress(&second);
        let HistoryOwnerDecisionV1::Move { movement, .. } = owner_request_v1(&progress).unwrap() else { panic!("first demand") };
        let mut row =
            PalwHistoryRecoveryV1::new_v1(progress.initial.clone(), 110, 400, 450, 20, 1, [bond(8)].into_iter().collect()).unwrap();
        apply(&mut row, &movement, 110);
        let mut old_begin = None;
        for record in [&second, &first] {
            let raw = borsh::to_vec(record).unwrap();
            while row.demand.is_some() {
                // A NEW actor sees only the accepted native state on every turn.
                row = borsh::from_slice(&borsh::to_vec(&row).unwrap()).unwrap();
                progress.session = Some(Box::new(row.clone()).into());
                progress.now_daa = 111;
                let (movement, until) = response_v1(&progress, bond(8), &raw).unwrap().unwrap();
                assert_eq!(until, 130);
                if old_begin.is_none() {
                    old_begin = Some(movement.clone());
                }
                assert!(queued_response_current_v1(&progress, bond(8), &movement));
                apply(&mut row, &movement, 111);
                progress.session = Some(Box::new(row.clone()).into());
                assert!(!queued_response_current_v1(&progress, bond(8), &movement));
            }
            if !row.complete_v1() {
                progress.session = Some(Box::new(row.clone()).into());
                let HistoryOwnerDecisionV1::Move { movement, .. } = owner_request_v1(&progress).unwrap() else {
                    panic!("next demand")
                };
                apply(&mut row, &movement, 110);
                assert!(old_begin.as_ref().unwrap().check_history_scope_v1(&row).is_err());
            }
        }
        progress.session = Some(Box::new(row).into());
        assert!(matches!(owner_request_v1(&progress).unwrap(), HistoryOwnerDecisionV1::Recovered));
        assert!(response_v1(&progress, bond(8), &borsh::to_vec(&first).unwrap()).unwrap().is_none());
    }
    #[test]
    fn peer_absence_requires_accepted_demand_and_collect_is_exact_once_da_only() {
        let record = record(Hash64::default(), 1);
        let mut progress = progress(&record);
        let mut row =
            PalwHistoryRecoveryV1::new_v1(progress.initial.clone(), 110, 400, 450, 20, 1, [bond(8)].into_iter().collect()).unwrap();
        let HistoryOwnerDecisionV1::Move { movement, .. } = owner_request_v1(&progress).unwrap() else { panic!("native demand") };
        apply(&mut row, &movement, 110);
        progress.session = Some(Box::new(row.clone()).into());
        progress.now_daa = 129;
        assert!(matches!(owner_request_v1(&progress).unwrap(), HistoryOwnerDecisionV1::Wait));
        progress.now_daa = 130;
        assert!(response_v1(&progress, bond(8), &borsh::to_vec(&record).unwrap()).unwrap().is_none());
        let HistoryOwnerDecisionV1::Move { movement, until_daa } = owner_request_v1(&progress).unwrap() else {
            panic!("expired demand collection")
        };
        assert_eq!(until_daa, 450);
        let PalwHistoryRecoveryEffectV1::Default(default) = apply(&mut row, &movement, 130) else { panic!("DA default only") };
        assert_eq!(default.custodians, [bond(8)].into_iter().collect());
        progress.session = Some(Box::new(row).into());
        assert!(matches!(owner_request_v1(&progress).unwrap(), HistoryOwnerDecisionV1::Default));
        assert_ne!(queue_scope_v1(&progress, bond(8)), queue_scope_v1(&progress, bond(9)));
    }
    #[test]
    fn grant_carrier_binds_own_valid_receipt_pledge_and_original_acceptance_clock() {
        use kaspa_consensus_core::palw_panel_v2::{PalwReceiptVerdictV2, PalwSeatReceiptV2};
        let duty = kaspa_consensus_core::palw_staged_court_state_v1::PalwHistoryGrantDutyV1 {
            snapshot_block: BlockHash::from_u64_word(1),
            now_daa: 120,
            claim: Hash64::from_u64_word(5),
            pledge_id: Hash64::from_u64_word(6),
            accepted_daa: 100,
            material_until_daa: 900,
        };
        let receipt = PalwSeatReceiptV2 {
            claim: duty.claim,
            verdict: PalwReceiptVerdictV2::Valid,
            seat_bond: bond(8),
            signed_daa: 110,
            signature: vec![3; kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
        };
        let (grant, movement) = grant_movement_v1(bond(8), &receipt, &duty).unwrap();
        assert_eq!((movement.reserver, movement.reserved_daa, movement.seed_id), (bond(8), 100, duty.pledge_id));
        assert_eq!((grant.claim, grant.seat, grant.signed_daa, grant.pledge_id), (duty.claim, bond(8), 110, duty.pledge_id));
        assert_eq!(movement.authorization_bond_v1(), bond(8));
        assert!(grant_movement_v1(bond(9), &receipt, &duty).is_err(), "another seat's receipt is no grant");
        let mut invalid = receipt.clone();
        invalid.verdict = PalwReceiptVerdictV2::Incapable;
        assert!(grant_movement_v1(bond(8), &invalid, &duty).is_err());
    }
    #[test]
    fn wrong_rooted_raw_or_unqualified_membership_is_not_a_response_credential() {
        let record = record(Hash64::default(), 1);
        let mut progress = progress(&record);
        let mut row =
            PalwHistoryRecoveryV1::new_v1(progress.initial.clone(), 110, 400, 450, 20, 1, [bond(8)].into_iter().collect()).unwrap();
        row.apply_v1(&PalwHistoryRecoveryActionV1::Demand, 110).unwrap();
        progress.now_daa = 111;
        progress.session = Some(Box::new(row).into());
        let mut raw = borsh::to_vec(&record).unwrap();
        raw[0] ^= 1;
        assert!(response_v1(&progress, bond(8), &raw).is_err());
        assert!(response_v1(&progress, bond(9), &raw).unwrap().is_none());
        assert!(
            response_v1(&progress, bond(2), &borsh::to_vec(&record).unwrap()).unwrap().is_some(),
            "independent owner may answer public bytes without custody liability"
        );
    }
}
