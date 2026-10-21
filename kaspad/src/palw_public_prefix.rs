//! Accepted-cursor public prefix acquisition. No producer capture enters the requester's
//! view: only authenticated binding/job, immutable semantics and rooted court progress do.
use crate::palw_panel::court_outbox::CourtOutboxV1;
use kaspa_consensus_core::{
    Hash64,
    palw_constraint_stream_carriage_v1::{
        PalwAuxiliaryPublicCourtSeedV1, PalwConstraintStreamActionV1, PalwConstraintStreamMoveV1, PalwConstraintStreamSeedV1,
    },
    palw_constraint_stream_court_v1::PalwConstraintStreamPhaseV1,
    palw_freeprompt_v3::PalwFreePromptJobV3,
    palw_offence_attribution_v1::{PalwOffenceTargetV1, palw_constrained_job_target_v1},
    palw_prefix_family_court_v1::{PalwPrefixCourtActionV1, PalwPrefixFamilyCursorV1},
    palw_prefix_public_source_v1::{
        PALW_PREFIX_PUBLIC_PART_BYTES_V1, PalwPrefixPublicAnswerActionV1, PalwPrefixPublicAnswerUnitV1, PalwPrefixPublicAnswerV1,
        PalwPrefixPublicSignerCursorV1,
    },
    palw_staged_court_state_v1::{PalwStagedCertificateRefV1, PalwStagedOwnedCursorV1, PalwStagedOwnedProgressV1},
    palw_state_v2::PalwBondKeyV2,
    palw_step_leg::{PalwStepBindingV2, PalwStepOpeningV1},
    palw_step_refute::PalwTiledDecodePinV1,
};
#[path = "palw_prefix_owned_pages.rs"]
mod owned_pages;
#[path = "palw_prefix_page_recovery.rs"]
mod page_recovery;

pub(super) enum PrefixScheduleV1 {
    Seed(PalwConstraintStreamSeedV1),
    Move(PalwConstraintStreamMoveV1),
    Finished(Option<PalwStagedCertificateRefV1>),
    Wait,
}

#[derive(Default)]
pub(super) struct PrefixCacheGcV1 {
    after: Option<(u64, Hash64)>,
    irreversible_daa: u64,
    queued: std::collections::VecDeque<kaspa_consensus_core::palw_prefix_history_v1::PalwStagedCacheLeaseV1>,
}

impl PrefixCacheGcV1 {
    pub(super) async fn tick_v1(
        &mut self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        directory: &std::path::Path,
    ) -> Result<(), String> {
        let _scratch = crate::palw_memory_ledger::host_ledger_v1()
            .reserve(
                crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
                    role: "public-prefix-cache-gc",
                    class_id: Hash64::default(),
                    job: Hash64::default(),
                },
                1_048_576,
            )
            .map_err(|e| e.to_string())?;
        if self.queued.is_empty() {
            let after = self.after;
            let page = session.clone().spawn_blocking(move |c| c.palw_staged_cache_gc_page_v1(after, 64)).await?;
            if page.leases.len() > 64 {
                return Err("native cache GC metadata exceeds bounded page".into());
            }
            self.after = page.next;
            self.irreversible_daa = page.irreversible_daa;
            self.queued.extend(page.leases);
        }
        let Some(lease) = self.queued.pop_front() else { return Ok(()) };
        if self.irreversible_daa < lease.material_until_daa {
            return Ok(());
        }
        let directory = directory.to_path_buf();
        let worker_lease = lease.clone();
        let irreversible = self.irreversible_daa;
        let done = tokio::task::spawn_blocking(move || {
            gc_prefix_cache_v1(&directory, worker_lease.seed_id, worker_lease.material_until_daa, irreversible)
        })
        .await
        .map_err(|e| format!("bounded public cache GC worker failed: {e}"))??;
        if done {
            let accepted = session.clone().spawn_blocking(move |c| c.palw_staged_cache_gc_ack_v1(lease)).await?;
            if !accepted {
                self.after = None;
            }
        }
        // An incomplete lease remains native and is revisited on the next metadata scan.
        // Do not hold one large scope ahead of all later eligible paid scopes forever.
        Ok(())
    }
}

pub(super) fn gc_prefix_cache_v1(
    directory: &std::path::Path,
    seed: Hash64,
    material_until: u64,
    irreversible_daa: u64,
) -> Result<bool, String> {
    page_recovery::gc_prefix_cache_v1(directory, seed, material_until, irreversible_daa)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn request_with_pages_v1(
    target: &PalwOffenceTargetV1,
    binding: &PalwStepBindingV2,
    job: &PalwFreePromptJobV3,
    progress: &PalwStagedOwnedProgressV1,
    maximum_leaves: u64,
    backend: Option<&dyn kaspa_consensus_core::palw_backend::PalwExecutionBackendV1>,
    directory: &std::path::Path,
    preview: impl FnOnce(kaspa_consensus_core::palw_prefix_gdn_paged_v1::PalwGdnOwnedPageOpeningV1) -> Result<Vec<i32>, String>,
    mut recover: impl FnMut() -> Result<bool, String>,
) -> Result<PrefixScheduleV1, String> {
    let Some(PalwStagedOwnedCursorV1::PublicPrefix { cursor, pending: None, completed: None, .. }) = &progress.cursor else {
        return request_v1(target, binding, job, progress, maximum_leaves, backend);
    };
    let (index, root, write, digest) = match cursor {
        PalwPrefixFamilyCursorV1::HeadPage { index, root, write, .. } => (*index, *root, *write, false),
        PalwPrefixFamilyCursorV1::HeadPageDigest { index, root, .. } => (*index, *root, false, true),
        _ => return request_v1(target, binding, job, progress, maximum_leaves, backend),
    };
    palw_constrained_job_target_v1(target, binding, job).map_err(|e| e.to_string())?;
    if progress.claim != target.claim_id
        || progress.reserver == target.executor_bond
        || progress.now_daa < progress.reserved_daa
        || progress.reserved_daa < progress.terms.accepted_daa
        || progress.now_daa >= progress.terms.stream_until_daa
        || target.registered_semantics.as_ref().and_then(|s| s.prefix_state_relation)
            != Some(kaspa_consensus_core::palw_class_semantics_v1::PalwPrefixStateRelationV1::Qwen36HybridCacheV1)
    {
        return Err("owned page continuation changes accepted public identity/relation/clock".into());
    }
    let seed_id = progress.seed_id.ok_or("accepted owned page lacks seed")?;
    let pages = match owned_pages::OwnerPagesV1::open_for_seed(
        directory,
        seed_id,
        binding.shape_profile.gdn_head_k_dim,
        binding.shape_profile.gdn_head_v_dim,
        progress.terms.plan.maximum_moves,
    ) {
        Ok(pages) => pages,
        Err(why) if why == "public owner page inventory pending" => return Ok(PrefixScheduleV1::Wait),
        Err(_) => {
            recover()?;
            return Ok(PrefixScheduleV1::Wait);
        }
    };
    let opening = match pages.opening(root, index) {
        Ok(opening) => opening,
        Err(_) => {
            recover()?;
            return Ok(PrefixScheduleV1::Wait);
        }
    };
    let action = if write {
        let values = preview(opening.clone())?;
        if pages.save_successor(index, &opening, &values).is_err() {
            recover()?;
            return Ok(PrefixScheduleV1::Wait);
        }
        PalwPrefixCourtActionV1::HeadPageWrite { opening }
    } else if digest {
        PalwPrefixCourtActionV1::HeadPageDigest { opening }
    } else {
        PalwPrefixCourtActionV1::HeadPageRead { opening }
    };
    Ok(PrefixScheduleV1::Move(PalwConstraintStreamMoveV1 {
        version: 1,
        claim: progress.claim,
        reserver: progress.reserver,
        reserved_daa: progress.reserved_daa,
        seed_id,
        phase: PalwConstraintStreamPhaseV1::PrefixCourt,
        action: PalwConstraintStreamActionV1::Prefix(action),
        signature: Vec::new(),
    }))
}

pub(super) fn needs_native_recovery_v1(directory: &std::path::Path, seed: Hash64) -> bool {
    page_recovery::needs_native_recovery_v1(directory, seed)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn recover_pages_with_public_pull_v1(
    consensus: &dyn kaspa_consensus_core::api::ConsensusApi,
    target: &PalwOffenceTargetV1,
    binding: &PalwStepBindingV2,
    job: &PalwFreePromptJobV3,
    progress: &PalwStagedOwnedProgressV1,
    maximum_leaves: u64,
    directory: &std::path::Path,
    pull: impl FnMut(
        &kaspa_consensus_core::palw_prefix_history_v1::PalwPrefixHistoryRequestV1,
    ) -> Result<Option<kaspa_consensus_core::palw_prefix_history_v1::PalwPrefixHistoryRecordV1>, String>,
) -> Result<bool, String> {
    page_recovery::recover_v1(consensus, target, binding, job, progress, maximum_leaves, directory, pull)
}

pub(super) fn request_v1(
    target: &PalwOffenceTargetV1,
    binding: &PalwStepBindingV2,
    job: &PalwFreePromptJobV3,
    progress: &PalwStagedOwnedProgressV1,
    maximum_leaves: u64,
    backend: Option<&dyn kaspa_consensus_core::palw_backend::PalwExecutionBackendV1>,
) -> Result<PrefixScheduleV1, String> {
    palw_constrained_job_target_v1(target, binding, job).map_err(|e| e.to_string())?;
    if progress.claim != target.claim_id
        || progress.reserver == target.executor_bond
        || progress.now_daa < progress.reserved_daa
        || progress.reserved_daa < progress.terms.accepted_daa
    {
        return Err("public prefix snapshot changes accepted independent ownership".into());
    }
    if progress.now_daa >= progress.terms.collect_until_daa {
        return Err("public prefix collection clock closed".into());
    }
    let named = kaspa_consensus_core::palw_fp_prefix_v1::palw_fp_prefix_shape_v1(job).map_err(|e| e.to_string())?;
    let relation = target
        .registered_semantics
        .as_ref()
        .and_then(|s| s.prefix_state_relation)
        .ok_or("public prefix lacks immutable registered relation")?;
    relation.check_profile(&binding.shape_profile)?;
    let Some(cursor) = &progress.cursor else {
        if progress.seed_id.is_some() || progress.now_daa >= progress.terms.open_until_daa {
            return Err("public prefix immutable opening clock closed".into());
        }
        let custodians = progress.prefix_custodians.as_ref().ok_or("public prefix lacks signed prospective custody")?;
        if custodians.contains(&progress.reserver) || !custodians.contains(&target.executor_bond) {
            return Err("public prefix owner is not outside literal custody".into());
        }
        let slots = custodians.len().checked_add(1).and_then(|n| u32::try_from(n).ok()).ok_or("prefix slots overflow")?;
        let empty = || PalwStepOpeningV1 { leaf_index: 0, leaf_hash: Hash64::default(), siblings: Vec::new() };
        let mut seed = PalwConstraintStreamSeedV1 {
            version: 1,
            claim: progress.claim,
            reserver: progress.reserver,
            reserved_daa: progress.reserved_daa,
            binding: binding.clone(),
            job: None,
            pin: PalwTiledDecodePinV1 {
                position: 0,
                generated_token_ids: Vec::new(),
                row_root: Hash64::default(),
                row_opening: empty(),
                committed_tile_lanes: Vec::new(),
                committed_opening: empty(),
                beat_tile_lanes: Vec::new(),
                beat_opening: empty(),
                beat_lane: 0,
            },
            witness: None,
            eog_count: 0,
            eog_opening: empty(),
            auxiliary: Some(PalwAuxiliaryPublicCourtSeedV1::PublicPrefix {
                maximum_leaves,
                prefix_tokens: named.prefix_tokens,
                relation,
                slots,
            }),
            signature: Vec::new(),
            retained_profile: None,
        };
        if borsh::object_length(&seed)
            .map_err(|e| e.to_string())?
            .checked_add(kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN)
            .ok_or("prefix seed carrier overflow")?
            > 90_000
        {
            seed.use_retained_profile_v1()?;
        }
        return Ok(PrefixScheduleV1::Seed(seed));
    };
    let seed_id = progress.seed_id.ok_or("accepted prefix cursor lacks seed identity")?;
    let PalwStagedOwnedCursorV1::PublicPrefix { cursor, demand, pending, completed, .. } = cursor else {
        return Err("reservation already owns another typed court".into());
    };
    if let Some(fault) = completed {
        if pending.is_some() || *cursor != PalwPrefixFamilyCursorV1::Complete {
            return Err("prefix completion has unfulfilled public continuation".into());
        }
        return Ok(PrefixScheduleV1::Finished(fault.then_some(PalwStagedCertificateRefV1 {
            claim: progress.claim,
            reserver: progress.reserver,
            reserved_daa: progress.reserved_daa,
            seed_id,
        })));
    }
    let action = if let Some(pending) = pending {
        if progress.now_daa < pending.deadline_daa {
            return Ok(PrefixScheduleV1::Wait);
        }
        PalwConstraintStreamActionV1::PrefixPublicCollect { cursor_id: pending.cursor_id, demand_id: pending.demand_id }
    } else if let Some(demand) = demand {
        PalwConstraintStreamActionV1::PrefixPublicDemand(demand.clone())
    } else {
        if progress.now_daa >= progress.terms.stream_until_daa {
            return Err("paid prefix semantic continuation clock closed".into());
        }
        let action = match cursor {
            PalwPrefixFamilyCursorV1::PublicRow { length, offset, .. } => {
                let remaining = length.checked_sub(*offset).ok_or("prefix public row offset exceeds length")?;
                if remaining == 0 {
                    return Err("empty prefix public row continuation".into());
                }
                PalwPrefixCourtActionV1::BeginPublicRow { length: remaining.min(65_536) as u32 }
            }
            PalwPrefixFamilyCursorV1::HeadParameters { layer, head } => {
                let backend = backend.ok_or("prefix verifier does not hold registered public head parameters")?;
                let node = binding
                    .shape_profile
                    .gdn_nodes
                    .iter()
                    .find(|n| {
                        n.kernel_semantics_id
                            == kaspa_consensus_core::palw_step::kernel_semantics_id_v1(
                                kaspa_consensus_core::palw_step_refute::KDESC_Q36_GDN_STEP,
                            )
                    })
                    .ok_or("registered head has no recurrence node")?;
                let offset = head.checked_mul(68).ok_or("head parameter offset overflow")?;
                let inventory = backend.artifact_inventory_digest()?;
                if inventory.root() != target.artifact_root {
                    return Err("own public artifact root differs from claim".into());
                }
                let index = inventory
                    .rows()
                    .iter()
                    .position(|row| {
                        row.tensor_name == node.weight_name
                            && row.layer == Some(*layer)
                            && row.row_start == offset
                            && row.byte_len == 68
                    })
                    .ok_or("own public artifact lacks canonical head parameter leaf")?;
                let parameters = vec![backend.artifact_row_opening(u32::try_from(index).map_err(|_| "artifact index overflow")?)?];
                if kaspa_consensus_core::palw_prefix_resources_v1::palw_prefix_gdn_mode_v1(&binding.shape_profile).0
                    == kaspa_consensus_core::palw_prefix_resources_v1::PalwPrefixGdnModeV1::Whole
                {
                    PalwPrefixCourtActionV1::BeginHead { parameters }
                } else {
                    PalwPrefixCourtActionV1::BeginPagedHead { parameters }
                }
            }
            PalwPrefixFamilyCursorV1::HeadDigest { length, offset, .. } => {
                let remaining = length.checked_sub(*offset).ok_or("head digest offset exceeds length")?;
                if remaining == 0 {
                    return Err("empty head digest continuation".into());
                }
                PalwPrefixCourtActionV1::HeadDigest { length: remaining.min(65_536) as u32 }
            }
            // Page openings belong to the owner and require reconstruction of its own
            // recurrence pages, not a producer secret snapshot. Never label this Honest.
            _ => return Err("public prefix requires bounded owner page reconstruction".into()),
        };
        PalwConstraintStreamActionV1::Prefix(action)
    };
    Ok(PrefixScheduleV1::Move(PalwConstraintStreamMoveV1 {
        version: 1,
        claim: progress.claim,
        reserver: progress.reserver,
        reserved_daa: progress.reserved_daa,
        seed_id,
        phase: PalwConstraintStreamPhaseV1::PrefixCourt,
        action,
        signature: Vec::new(),
    }))
}

/// One bounded carrier from an exact original leaf. The caller reserves the full held
/// material before generating it. Byte offsets and retry identities come only from the
/// accepted pending slot, never from a locally remembered send or caller coordinate.
pub(super) fn answer_v1(
    progress: &PalwStagedOwnedProgressV1,
    signer: PalwBondKeyV2,
    binding: &PalwStepBindingV2,
    tile: &kaspa_consensus_core::palw_prefix_state_court_v1::PalwPrefixKvTileV1,
) -> Result<PalwConstraintStreamMoveV1, String> {
    let Some(PalwStagedOwnedCursorV1::PublicPrefix { pending: Some(pending), .. }) = &progress.cursor else {
        return Err("public prefix responder has no accepted pending source".into());
    };
    if progress.now_daa < progress.reserved_daa || progress.now_daa >= pending.deadline_daa {
        return Err("public prefix responder clock closed".into());
    }
    let derived = kaspa_consensus_core::palw_prefix_public_source_v1::PalwPrefixPublicSourceV1::derive(
        binding,
        pending.source.coordinate,
        pending.source.maximum_leaves,
    )?;
    if derived != pending.source
        || tile.preimage.coord != pending.source.coordinate
        || tile.preimage.value_count != pending.source.value_count
        || tile.preimage.values_le.len() != pending.source.byte_length as usize
        || tile.opening.leaf_index != pending.source.leaf_index
        || kaspa_consensus_core::palw_step_leg::step_tile_leaf_hash_ctx_v1(
            &binding.job_context,
            &binding.shape_profile.shape_profile_id(),
            &tile.preimage,
        ) != tile.opening.leaf_hash
        || kaspa_consensus_core::palw_step_leg::step_opening_root_capped_v1(
            binding.step_leaf_count,
            &tile.opening,
            pending.source.maximum_leaves,
        )
        .map_err(|e| e.to_string())?
            != binding.step_merkle_root
    {
        return Err("public prefix responder material is not the exact authenticated original leaf".into());
    }
    let slot = pending.signers.get(&signer).ok_or("responder is outside literal signed custodian slots")?;
    let mut units = Vec::new();
    let offset = match slot {
        PalwPrefixPublicSignerCursorV1::Opening => {
            units.push(PalwPrefixPublicAnswerUnitV1::Begin { opening: tile.opening.clone() });
            0
        }
        PalwPrefixPublicSignerCursorV1::Bytes { offset } => *offset,
        PalwPrefixPublicSignerCursorV1::Finish => pending.source.byte_length,
        PalwPrefixPublicSignerCursorV1::Invalid => return Err("responder's one paid original-leaf attempt exhausted".into()),
    };
    if offset < pending.source.byte_length {
        let end = (offset as usize + PALW_PREFIX_PUBLIC_PART_BYTES_V1).min(tile.preimage.values_le.len());
        units.push(PalwPrefixPublicAnswerUnitV1::Part { offset, bytes: tile.preimage.values_le[offset as usize..end].to_vec() });
        if end == tile.preimage.values_le.len() {
            units.push(PalwPrefixPublicAnswerUnitV1::Finish);
        }
    } else {
        units.push(PalwPrefixPublicAnswerUnitV1::Finish);
    }
    let answer = PalwPrefixPublicAnswerV1 {
        answerer: signer,
        cursor_id: pending.cursor_id,
        demand_id: pending.demand_id,
        action: PalwPrefixPublicAnswerActionV1::Batch(units),
    };
    answer.check()?;
    Ok(PalwConstraintStreamMoveV1 {
        version: 1,
        claim: progress.claim,
        reserver: progress.reserver,
        reserved_daa: progress.reserved_daa,
        seed_id: progress.seed_id.ok_or("accepted prefix pending lacks seed")?,
        phase: PalwConstraintStreamPhaseV1::PrefixCourt,
        action: PalwConstraintStreamActionV1::PrefixPublicAnswer(answer),
        signature: Vec::new(),
    })
}

impl super::PalwPanelService {
    pub(super) async fn public_prefix_answer_tick_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        slot: super::PalwAnswerSlotV1<'_>,
        cursor: &mut Option<(u64, Hash64, PalwBondKeyV2)>,
        court_outbox: &mut CourtOutboxV1,
    ) -> Result<(), String> {
        use super::*;
        // Distinct from the replay filer's demand round (`u32::MAX - 6`), which retains by round.
        const ROUND: u32 = u32::MAX - 20;
        let signer = slot.slot;
        let lane_round = ROUND - slot.round_offset;
        let page = session.palw_prefix_response_duties_v1(signer, *cursor)?;
        *cursor = page.next;
        // Remove a stale branch/fulfilled source before spending another fee input.
        court_outbox.retain_pending(|(_, round, responder, object)| {
            if (*round, *responder) != (lane_round, true) {
                return true;
            }
            let PalwConsensusObjectV2::PublicCourtStreamMoveV1 { movement } = object else {
                return false;
            };
            if let PalwConstraintStreamActionV1::PrefixPublicAnswer(a) = &movement.action
                && a.answerer != signer
            {
                return true;
            }
            // Page omission is not fulfilment. Check each bounded queued carrier against
            // its direct accepted row so pagination cannot discard another page's work.
            match session.palw_staged_owned_progress_v1((movement.claim, movement.reserver)) {
                Err(_) => true,
                Ok(None) => false,
                Ok(Some(progress)) => {
                    let Some(PalwStagedOwnedCursorV1::PublicPrefix { pending: Some(p), .. }) = &progress.cursor else {
                        return false;
                    };
                    let PalwConstraintStreamActionV1::PrefixPublicAnswer(a) = &movement.action else {
                        return false;
                    };
                    let units = match &a.action {
                        PalwPrefixPublicAnswerActionV1::Batch(u) => u,
                        _ => return false,
                    };
                    let expected = match p.signers.get(&signer) {
                        Some(PalwPrefixPublicSignerCursorV1::Opening) => {
                            matches!(units.first(), Some(PalwPrefixPublicAnswerUnitV1::Begin { .. }))
                        }
                        Some(PalwPrefixPublicSignerCursorV1::Bytes { offset }) => {
                            matches!(units.first(), Some(PalwPrefixPublicAnswerUnitV1::Part { offset: at, .. }) if at == offset)
                        }
                        Some(PalwPrefixPublicSignerCursorV1::Finish) => {
                            matches!(units.first(), Some(PalwPrefixPublicAnswerUnitV1::Finish))
                        }
                        _ => false,
                    };
                    expected
                        && progress.now_daa < p.deadline_daa
                        && movement.claim == progress.claim
                        && movement.reserver == progress.reserver
                        && movement.reserved_daa == progress.reserved_daa
                        && Some(movement.seed_id) == progress.seed_id
                        && a.answerer == signer
                        && (a.cursor_id, a.demand_id) == (p.cursor_id, p.demand_id)
                }
            }
        });
        for duty in page.duties {
            if slot.only.is_some_and(|only| !only.contains(&duty.material.claim_id)) {
                continue;
            }
            if court_outbox.pending().iter().filter(|(_, round, responder, _)| (*round, *responder) == (lane_round, true)).count()
                >= 64
            {
                break;
            }
            let Some(seed_id) = duty.progress.seed_id else { continue };
            let key = (seed_id, lane_round, true);
            if court_outbox.pending().iter().any(|(a, b, c, _)| (*a, *b, *c) == key)
                || court_outbox.submitted_at(&key).is_some_and(|at| duty.progress.now_daa < at.saturating_add(COURT_MOVE_REPLAN_DAA))
            {
                continue;
            }
            let claim = duty.material.claim_id;
            let deadline = duty.material.deadline_daa;
            let now = duty.progress.now_daa;
            // Refused holdings/backend/worker resources affect only this literal job;
            // remaining indexed duties still receive their independent protected turn.
            let result = async {
                self.foreign_pinned.lock().unwrap().insert(duty.progress.claim);
                let material = &duty.material;
                let need = self
                    .backends()
                    .role_memory_need_or_chain_v1(material.class_id, material.artifact_root, FULL_SEAT_V1, |id| {
                        self.chain_carriage_v1(session, id)
                    })
                    .ok_or("prefix responder resource profile unavailable")?;
                let reservation = self.reserve_replay_v1("public-prefix-answer", &need, material.class_id, material.claim_id)?;
                let Some(PalwStagedOwnedCursorV1::PublicPrefix { pending: Some(pending), .. }) = &duty.progress.cursor else {
                    return Err("prefix responder lost accepted pending source".into());
                };
                // FullSeat owns backend residency and complete retained/re-made capture work.
                // The independent transport guard covers eight original-leaf copies, metadata,
                // CKW encodings and bounded signed packets while moving into the ordinary queue.
                let transport_bytes = u64::from(pending.source.byte_length)
                    .checked_mul(8)
                    .and_then(|n| (borsh::object_length(&pending).ok()? as u64).checked_mul(8).and_then(|m| n.checked_add(m)))
                    .and_then(|n| self.config.court.max_close_bytes().checked_mul(4).and_then(|m| n.checked_add(m)))
                    .and_then(|n| n.checked_add(360_000))
                    .ok_or("prefix responder transport bound overflow")?;
                let transport = crate::palw_memory_ledger::host_ledger_v1()
                    .reserve(
                        crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
                            role: "public-prefix-answer-carrier",
                            class_id: material.class_id,
                            job: material.claim_id,
                        },
                        transport_bytes,
                    )
                    .map_err(|e| e.to_string())?;
                let backend: std::sync::Arc<dyn kaspa_consensus_core::palw_backend::PalwExecutionBackendV1> =
                    if material.executor_bond == signer && slot.only.is_none() {
                        self.executor_backend_v1(session, material.class_id, material.artifact_root)?
                    } else {
                        std::sync::Arc::from(self.resolve_backend(session, material.class_id, material.artifact_root)?)
                    };
                let (_, _, work_leaves) =
                    session.palw_claim_roots_v2(material.claim_id).ok_or("prefix answer claim roots unavailable")?;
                let facts = PalwDaClaimFactsV1 {
                    claim_id: material.claim_id,
                    class_id: material.class_id,
                    executor_bond: material.executor_bond,
                    execution_root: material.execution_root,
                    trace_root: material.trace_root,
                    work_leaves,
                    form: self.class_prompt_ids_form(material.class_id),
                    lane: PalwDaLaneV1::FreePrompt { panel_da_admissible: self.consensus_config.params.palw_panel_da_admissible() },
                    job_pin: material.fp_job_pin_v1(),
                };
                let paths = self.fp_retained_payload_paths(&material.claim_id).to_vec();
                let foreign = self.config.retention_dir.join("foreign");
                let claim = material.claim_id;
                let unit = material.unit;
                let progress = duty.progress;
                let keypair = self.keypair.clone();
                tokio::task::spawn_blocking(move || -> Result<PalwConsensusObjectV2, String> {
                    let _reservation = reservation;
                    let _transport = transport;
                    let mut built = palw_da_claim_answers_v1(
                        backend.as_ref(),
                        &facts,
                        paths.iter().filter_map(|p| std::fs::read(p).ok()),
                        |bytes| palw_da_keep_remade_v1(&foreign, &claim, bytes),
                        &[unit],
                        0,
                        false,
                        None,
                    )?;
                    let PalwDaBuiltAnswerV1::LegacyHeldV2(held) =
                        built.answers.pop().flatten().ok_or("prefix source was not produced")??
                    else {
                        return Err("prefix source is not original held witness".into());
                    };
                    let (binding, witness) = *held;
                    let kaspa_consensus_core::palw_legacy_held_da_v2::PalwLegacyHeldAnswerV2::KernelWitness(witness) = witness else {
                        return Err("prefix source is not committed kernel witness".into());
                    };
                    let tile = kaspa_consensus_core::palw_prefix_state_court_v1::PalwPrefixKvTileV1 {
                        opening: witness.refutation.output_opening,
                        preimage: witness.refutation.output_preimage,
                    };
                    let mut movement = answer_v1(&progress, signer, &binding, &tile)?;
                    movement.signature = keypair
                        .as_ref()
                        .and_then(|kp| {
                            libcrux_ml_dsa::ml_dsa_87::sign(
                                &kp.signing_key,
                                &movement.signing_message(),
                                kaspa_consensus_core::palw_constraint_stream_carriage_v1::PALW_CONSTRAINT_STREAM_CONTEXT_V1,
                                [0u8; 32],
                            )
                            .ok()
                            .map(|s| s.as_ref().to_vec())
                        })
                        .ok_or("prefix responder signing key unavailable")?;
                    movement.check()?;
                    let object = PalwConsensusObjectV2::PublicCourtStreamMoveV1 { movement: Box::new(movement) };
                    if borsh::object_length(&object).map_err(|e| e.to_string())? > 90_000 {
                        return Err("prefix response exceeds ordinary signed carrier".into());
                    }
                    Ok(object)
                })
                .await
                .map_err(|e| format!("prefix source worker failed: {e}"))?
            }
            .await;
            match result {
                Ok(object) => {
                    court_outbox.set_due(key, deadline.saturating_sub(1));
                    court_outbox.push_pending((key.0, key.1, key.2, object));
                }
                Err(why) => {
                    court_outbox.mark_submitted(key, now);
                    debug!("[{PALW_PANEL}] claim {claim}: public prefix source remains unavailable: {why}");
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
    use kaspa_consensus_core::palw_prefix_public_source_v1::{PalwPrefixPublicPendingV1, PalwPrefixPublicSourceV1};
    fn bond(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint {
            transaction_id: kaspa_consensus_core::tx::TransactionId::from_u64_word(n),
            index: 0,
        })
    }
    #[test]
    fn responder_uses_accepted_literal_slot_and_original_leaf_across_restart() {
        let backend = super::super::seat_s_tests::floor_backend();
        let (job, ids) = super::super::seat_s_tests::floor_fp_job(&backend);
        let run = backend.execute_free_prompt(&job, &ids.iter().map(|n| *n as usize).collect::<Vec<_>>()).unwrap();
        let witness = backend.refutation_for_free_prompt_index(&run.outcome.material, 0, &ids).unwrap();
        let binding = witness.binding;
        let tile = kaspa_consensus_core::palw_prefix_state_court_v1::PalwPrefixKvTileV1 {
            opening: witness.output_opening,
            preimage: witness.output_preimage,
        };
        let source = PalwPrefixPublicSourceV1::derive(&binding, tile.preimage.coord, 1 << 40).unwrap();
        let mut pending = PalwPrefixPublicPendingV1::new(
            Hash64::from_u64_word(1),
            bond(7),
            100,
            Hash64::from_u64_word(2),
            Hash64::from_u64_word(3),
            Hash64::from_u64_word(4),
            100,
            200,
            source,
            [bond(1), bond(2)].into_iter().collect(),
        )
        .unwrap();
        let prices = kaspa_consensus_core::palw_rendering_resources_v1::PalwStagedPublicCourtConfigV1::network_v1().prices;
        let plan = kaspa_consensus_core::palw_rendering_resources_v1::PalwRenderingResourcePlanV1::derive(1, 1, prices).unwrap();
        let mut progress = PalwStagedOwnedProgressV1 {
            snapshot_block: kaspa_consensus_core::BlockHash::from_u64_word(5),
            now_daa: 100,
            claim: Hash64::from_u64_word(1),
            reserver: bond(7),
            reserved_daa: 100,
            terms: kaspa_consensus_core::palw_rendering_resources_v1::PalwRenderingClaimTermsV1::derive(100, plan, 100, 100, 10)
                .unwrap(),
            prefix_custodians: Some([bond(1), bond(2)].into_iter().collect()),
            prefix_page_root: None,
            bundle: None,
            seed_id: Some(Hash64::from_u64_word(2)),
            cursor: Some(PalwStagedOwnedCursorV1::PublicPrefix {
                cursor: PalwPrefixFamilyCursorV1::TileOpening { ordinal: 0 },
                history_count: 0,
                history_head: Hash64::default(),
                demand: None,
                pending: Some(pending.progress_v1()),
                completed: None,
            }),
        };
        let first = answer_v1(&progress, bond(1), &binding, &tile).unwrap();
        assert_eq!(first, answer_v1(&progress, bond(1), &binding, &tile).unwrap(), "a queued send is not acceptance");
        assert_eq!(first.authorization_bond_v1(), bond(1), "signature is custodian's, not owner's");
        assert!(answer_v1(&progress, bond(99), &binding, &tile).is_err());
        let mut forged = tile.clone();
        forged.preimage.values_le[0] ^= 1;
        assert!(answer_v1(&progress, bond(1), &binding, &forged).is_err());
        // Fold only Begin, lose all local scheduling state, and regenerate the next packet
        // solely from the exact accepted slot. No producer cache checkpoint is supplied.
        pending.begin(bond(1), 100, &tile.opening).unwrap();
        if let Some(PalwStagedOwnedCursorV1::PublicPrefix { pending: view, .. }) = &mut progress.cursor {
            *view = Some(pending.progress_v1());
        }
        let restarted = answer_v1(&progress, bond(1), &binding, &tile).unwrap();
        let PalwConstraintStreamActionV1::PrefixPublicAnswer(answer) = &restarted.action else { panic!() };
        let PalwPrefixPublicAnswerActionV1::Batch(units) = &answer.action else { panic!() };
        assert!(matches!(units.first(), Some(PalwPrefixPublicAnswerUnitV1::Part { offset: 0, .. })));
        pending.packet(bond(1), 100, &answer.action).unwrap();
        assert_eq!(pending.authenticated_answer().unwrap().1, tile.preimage.values_le);
        progress.now_daa = 200;
        assert!(answer_v1(&progress, bond(1), &binding, &tile).is_err(), "deadline is not silently extended");
    }
}
