//! Historical context comes from the executing fold, never a producer list.
//! Credit/frontier integration is deliberately separate from this journal writer.
use super::*;
use crate::palw_claim_local_v1::*;

impl PalwChainStateV2 {
    pub(crate) fn claim_local_prefix_source_point_v1(
        &self,
        source: &crate::palw_claim_local_v1::prefix_v1::PalwPrefixExportIdentityV1,
    ) -> Result<PalwBlockContextV2, PalwStateV2Error> {
        if let Some(point) = super::palw_claim_local_source_admission_v8::point_for_prefix_identity_v8(self, source)? {
            return Ok(point);
        }
        let bad = || PalwStateV2Error::CarriageInconsistent("RFC0040 source birth is absent or differs".into());
        let key = palw_claim_liability_frame_key_v1(source.accepted_block);
        let bytes = self.kernel_route.as_ref().and_then(|route| {
            route.aux.iter().find_map(|((table, stored_key), value)| {
                (*table == PALW_CLAIM_LOCAL_TABLE_V1 && stored_key.as_slice() == key).then_some(value)
            })
        });
        let Some(bytes) = bytes else {
            let now = self.last_point.ok_or_else(bad)?.daa_score;
            return super::palw_tir_public_fold_v1::original_archive_reservation_proposal_v1::with_verified_ancestor_descriptor_v19(
                self,
                source.source_claim,
                source.accepted_block,
                source.accepted_daa,
                source.accepted_blue_score,
                now,
                |d| Ok(d.point_v19()),
            )?
            .ok_or_else(bad);
        };
        let frame = palw_decode_claim_liability_frame_v1(bytes).map_err(|error| match error {
            crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(error) => PalwStateV2Error::NativeResource(error),
            crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(_) => bad(),
        })?;
        let point = frame.point;
        if point.block != source.accepted_block
            || point.daa_score != source.accepted_daa
            || point.blue_score != source.accepted_blue_score
        {
            return Err(bad());
        }
        Ok(point)
    }
    /// Read a claim's admission fact from its actual accepting point. A later
    /// changed-claim frame never substitutes for the birth or for migration data.
    /// The state/root must already be authenticated by the normal node path.
    pub fn claim_local_admission_context_v1(
        &self,
        params: &PalwStateParamsV2,
        accepting: &PalwBlockContextV2,
        id: Hash64,
    ) -> Result<Option<PalwClaimLocalAdmissionContextV1>, PalwStateV2Error> {
        if !params.claim_local_settlement_active_at(accepting.daa_score)
            || self.last_point.is_none_or(|p| accepting.blue_score > p.blue_score || accepting.daa_score > p.daa_score)
        {
            return Ok(None);
        }
        if self.kernel_route.is_none() {
            return Ok(None);
        }
        let key = palw_claim_liability_frame_key_v1(accepting.block);
        let _key_lease =
            crate::palw_native_resource_v1::reserve_native_requests_v1(key.len() as u64).map_err(PalwStateV2Error::NativeResource)?;
        let Some(bytes) = self.kernel_route.as_ref().and_then(|r| r.aux.get(&(PALW_CLAIM_LOCAL_TABLE_V1, key.to_vec()))) else {
            return Ok(None);
        };
        let frame = palw_decode_claim_liability_frame_v1(bytes).map_err(|why| match why {
            crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(why) => PalwStateV2Error::NativeResource(why),
            crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(why) => {
                PalwStateV2Error::CarriageInconsistent(why.to_string())
            }
        })?;
        if frame.point != *accepting {
            return Err(PalwStateV2Error::CarriageInconsistent("claim admission frame differs from its accepting point".into()));
        }
        admission_from_native_retained_frame_v19(frame, accepting, id)
    }

    pub(super) fn assert_claim_local_context_v1(&self, params: &PalwStateParamsV2) -> Result<(), PalwStateV2Error> {
        super::palw_claim_local_legacy_terminal_v18::validate_profiles_v18(self)?;
        if self.last_point.is_some_and(|p| params.claim_local_settlement_active_at(p.daa_score)) {
            super::palw_claim_local_legacy_terminal_v18::validate_pending_v18(self, params)?;
        }
        super::palw_claim_local_native_mining_birth_v20::validate_native_receipts_v20(self, params)?;
        // RFC-0023 stage 2: the stored checkpoint equals the recomputation (and is absent below the fence).
        super::palw_claim_local_checkpoint_fold_v1::assert_claim_local_checkpoint_v1(self, params)?;
        if !self.last_point.is_some_and(|point| params.claim_local_settlement_active_at(point.daa_score)) {
            return Ok(());
        }
        let Some(route) = &self.kernel_route else {
            return Ok(());
        };
        for ((table, key), bytes) in &route.aux {
            if *table != PALW_CLAIM_LOCAL_TABLE_V1 {
                continue;
            }
            if key.first().is_some_and(|p| matches!(*p, b'N' | b'B' | b'I' | b'A')) {
                // The checkpoint assertion above validates scoped key shapes,
                // content references, archive authority and the actual fence.
                // These rows are distinct from the legacy F1 carriage namespace.
                continue;
            }
            if key.first().is_some_and(|p| super::palw_claim_local_descriptor_v2::is_prefix(*p)) {
                // The resolved descriptor has already checked every normalized
                // root, key, body identity and reference against this snapshot.
                super::palw_claim_local_descriptor_v2::validate_key(key)?;
                continue;
            }
            if key.first() == Some(&b'C') {
                // RFC-0023 stage 2 checkpoint: judged whole below, against a recomputation.
                if key.as_slice() != super::palw_claim_local_checkpoint_fold_v1::palw_claim_local_checkpoint_key_v1() {
                    return Err(PalwStateV2Error::CarriageInconsistent("bad claim-local checkpoint key".into()));
                }
                continue;
            }
            if key.first() == Some(&b'E') {
                let row = decode_claim_local_value_v1::<prefix_v1::PalwPrefixExportReservationV1>(bytes).map_err(|e| match e {
                    crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(e) => PalwStateV2Error::NativeResource(e),
                    crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(_) => {
                        PalwStateV2Error::CarriageInconsistent("malformed prefix reservation".into())
                    }
                })?;
                if key.len() != 65
                    || key[1..] != *row.budget_id.as_byte_slice()
                    || self.prefix_export_reservation_v1(&row.point, row.source_claim)?.as_ref() != Some(&*row)
                {
                    return Err(PalwStateV2Error::CarriageInconsistent("prefix reservation identity differs".into()));
                }
                // A retained exact A8 birth remains authority after its legacy
                // full F1 frame is collected. Use the native scoped reader and
                // its unchanged strict legacy fallback; never synthesize a frame.
                let origin =
                    super::palw_claim_local_source_origin_v8::SourceOriginV8::at_point(self, params, row.source_claim, &row.point)?
                        .ok_or_else(|| {
                            PalwStateV2Error::CarriageInconsistent("prefix reservation lost source birth material".into())
                        })?;
                if origin.original_claim().bond != row.bond {
                    return Err(PalwStateV2Error::CarriageInconsistent("prefix reservation changed source bond".into()));
                }
                let expiry = super::palw_claim_local_prefix_reservation_v1::expiry_key(row.material_until, row.budget_id);
                let indexed = route
                    .aux
                    .iter()
                    .any(|((t, k), v)| *t == PALW_CLAIM_LOCAL_TABLE_V1 && k.as_slice() == expiry && v.as_slice() == [1]);
                let material_pending = match self.last_point {
                    Some(p) => self.prefix_export_material_pending_v1(params, &row, p.daa_score)?,
                    None => false,
                };
                if indexed != material_pending {
                    return Err(PalwStateV2Error::CarriageInconsistent("prefix reservation expiry index disagrees".into()));
                }
                continue;
            }
            if key.first() == Some(&b'X') {
                if key.len() != 73 || bytes.as_slice() != [1] {
                    return Err(PalwStateV2Error::CarriageInconsistent("prefix expiry index differs".into()));
                }
                let id = Hash64::from_bytes(key[9..].try_into().expect("fixed index id"));
                let row_key = super::palw_claim_local_prefix_reservation_v1::row_key(id);
                let value = route
                    .aux
                    .iter()
                    .find_map(|((t, k), v)| (*t == PALW_CLAIM_LOCAL_TABLE_V1 && k.as_slice() == row_key).then_some(v))
                    .ok_or_else(|| PalwStateV2Error::CarriageInconsistent("prefix expiry has no reservation".into()))?;
                let row = decode_claim_local_value_v1::<prefix_v1::PalwPrefixExportReservationV1>(value).map_err(|e| match e {
                    crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(e) => PalwStateV2Error::NativeResource(e),
                    crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(_) => {
                        PalwStateV2Error::CarriageInconsistent("bad prefix expiry reservation".into())
                    }
                })?;
                if key.as_slice() != super::palw_claim_local_prefix_reservation_v1::expiry_key(row.material_until, row.budget_id) {
                    return Err(PalwStateV2Error::CarriageInconsistent("prefix expiry changed original deadline".into()));
                }
                continue;
            }
            if key.first() == Some(&b'R') {
                if key.len() != 65 {
                    return Err(PalwStateV2Error::CarriageInconsistent("bad reward hold key".into()));
                }
                let hold = crate::palw_claim_local_v1::decode_claim_local_value_v1::<PalwClaimLocalRewardHoldV1>(bytes).map_err(
                    |e| match e {
                        crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(e) => PalwStateV2Error::NativeResource(e),
                        crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(e) => {
                            PalwStateV2Error::CarriageInconsistent(e.to_string())
                        }
                    },
                )?;
                let id = Hash64::from_bytes(key[1..].try_into().expect("fixed key"));
                if self
                    .claims
                    .get(&id)
                    .is_none_or(|c| c.accepted_daa != hold.accepted_daa || c.accepted_blue_score != hold.accepted_blue_score)
                {
                    return Err(PalwStateV2Error::CarriageInconsistent("reward hold lost its claim birth".into()));
                }
                continue;
            }
            if key.first() == Some(&super::PALW_COMPACT_KERNEL_ORIGIN_PREFIX_V1) {
                if key.len() != 65 {
                    return Err(PalwStateV2Error::CarriageInconsistent("bad compact kernel origin key".into()));
                }
                let id = Hash64::from_bytes(key[1..].try_into().expect("compact kernel key"));
                let row = super::palw_claim_local_kernel_origins_v1::read(self, id)?
                    .ok_or_else(|| PalwStateV2Error::CarriageInconsistent("compact kernel origin disappeared".into()))?;
                if !params.claim_local_settlement_active_at(row.birth.daa_score) {
                    return Err(PalwStateV2Error::CarriageInconsistent("compact kernel origin precedes actual stage2 fence".into()));
                }
                continue;
            }
            if key.first() == Some(&super::PALW_COMPACT_CLAIM_SOURCE_PREFIX_V1) {
                if key.len() != 65 {
                    return Err(PalwStateV2Error::CarriageInconsistent("bad compact source key".into()));
                }
                let id = Hash64::from_bytes(key[1..].try_into().expect("fixed compact key"));
                let row = super::palw_claim_local_compact_sources_v1::read(self, id)?
                    .ok_or_else(|| PalwStateV2Error::CarriageInconsistent("compact source disappeared".into()))?;
                if self.last_point.is_none_or(|p| row.closed_at.daa_score > p.daa_score || row.closed_at.blue_score > p.blue_score)
                    || !params.claim_local_settlement_active_at(row.birth.daa_score)
                    || (row.permitted
                        && (!matches!(row.terminal_phase, PalwClaimPhaseV2::Final { .. })
                            || super::palw_claim_local_rewards_v1::approval(params, id, &row.original, Some(&row.approval))?
                                != super::palw_claim_local_rewards_v1::RewardReadinessV1::Ready))
                {
                    return Err(PalwStateV2Error::CarriageInconsistent(
                        "compact source closure/selected approval context differs".into(),
                    ));
                }
                continue;
            }
            // Aux144 is shared with native held-fee/material duty rows. They
            // have explicit typed codecs and identities; they are not F frames.
            if key.len() == 66 && key[0] == crate::palw_native_liability_hold_v1::PALW_NATIVE_HELD_FEE_PREFIX_V1 {
                let row = super::palw_claim_local_rewards_v1::decoded::<crate::palw_native_liability_hold_v1::PalwNativeHeldFeeRowV1>(
                    bytes,
                )?;
                if row.version != 1 || key[1] != row.family || key[2..] != row.job || row.horizon_daa < row.final_daa {
                    return Err(PalwStateV2Error::CarriageInconsistent("native held fee key/horizon differs".into()));
                }
                continue;
            }
            if key.len() == 66 && key[0] == super::palw_native_post_final_duty_v1::PALW_NATIVE_POST_FINAL_DUTY_PREFIX_V1 {
                let row = super::palw_claim_local_rewards_v1::decoded::<
                    crate::palw_native_post_final_duty_row_v1::PalwNativePostFinalDutyRowV1,
                >(bytes)?;
                if row.version != 1 || key[1] != row.family || key[2..] != row.job {
                    return Err(PalwStateV2Error::CarriageInconsistent("native post-Final duty key differs".into()));
                }
                continue;
            }
            if key.len() == 74 && key[0] == crate::palw_native_liability_hold_v1::PALW_NATIVE_HELD_FEE_HORIZON_PREFIX_V1 {
                let value = super::palw_claim_local_rewards_v1::decoded::<u8>(bytes)?;
                let horizon = u64::from_be_bytes(key[1..9].try_into().expect("fixed horizon key"));
                let mut held = [0; 66];
                held[0] = crate::palw_native_liability_hold_v1::PALW_NATIVE_HELD_FEE_PREFIX_V1;
                held[1] = key[9];
                held[2..].copy_from_slice(&key[10..]);
                let Some(raw) =
                    route.aux.iter().find(|((t, k), _)| *t == PALW_CLAIM_LOCAL_TABLE_V1 && k.as_slice() == held).map(|(_, v)| v)
                else {
                    return Err(PalwStateV2Error::CarriageInconsistent("native fee horizon lost held row".into()));
                };
                let row =
                    super::palw_claim_local_rewards_v1::decoded::<crate::palw_native_liability_hold_v1::PalwNativeHeldFeeRowV1>(raw)?;
                if *value != 1 || row.horizon_daa != horizon {
                    return Err(PalwStateV2Error::CarriageInconsistent("native fee horizon index differs".into()));
                }
                continue;
            }
            if key.len() == 130 && key[0] == super::palw_native_post_final_duty_v1::PALW_NATIVE_POST_FINAL_MATERIAL_PREFIX_V1 {
                let row = super::palw_claim_local_rewards_v1::decoded::<
                    crate::palw_native_post_final_duty_row_v1::PalwNativePostFinalMaterialRowV1,
                >(bytes)?;
                if row.version != 1 || key[1] != row.family || key[2..66] != row.job || key[66..] != row.commitment {
                    return Err(PalwStateV2Error::CarriageInconsistent("native post-Final material key differs".into()));
                }
                continue;
            }
            if key.len() != 65 || key[0] != b'F' {
                return Err(PalwStateV2Error::CarriageInconsistent("unallocated claim-local/native144 row namespace".into()));
            }
            let frame = palw_decode_claim_liability_frame_v1(bytes).map_err(|why| match why {
                crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(why) => PalwStateV2Error::NativeResource(why),
                crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(why) => {
                    PalwStateV2Error::CarriageInconsistent(why.to_string())
                }
            })?;
            if key.as_slice() != palw_claim_liability_frame_key_v1(frame.point.block)
                || self.last_point.is_none_or(|p| frame.point.blue_score > p.blue_score || frame.point.daa_score > p.daa_score)
                || !params.claim_local_settlement_active_at(frame.point.daa_score)
            {
                return Err(PalwStateV2Error::CarriageInconsistent("liability frame key/point differs".into()));
            }
            frame.validate_context_v1().map_err(|why| match why {
                PalwClaimCheckpointErrorV1::Resource(why) => PalwStateV2Error::NativeResource(why),
                PalwClaimCheckpointErrorV1::Semantic(why) => PalwStateV2Error::CarriageInconsistent(why.into()),
            })?;
        }
        Ok(())
    }
}

/// Source claims whose settled carrier transaction is spent by `spent` (consumer claim, spent txid).
///
/// The index is the winning-carrier aux row already stored on `parent`. A txid that names more
/// than one claim is skipped. This block's own new rows are not visible yet, and a claim does not
/// depend on itself.
pub(super) fn consumed_carrier_edges_v1(
    parent: &PalwChainStateV2,
    spent: impl IntoIterator<Item = (Hash64, Hash64)>,
) -> BTreeSet<(Hash64, Hash64)> {
    use crate::palw_fp_answer_accounting_v1::PALW_FP_WINNING_CARRIER_TABLE_V1;
    let Some(route) = parent.kernel_route.as_ref() else {
        return BTreeSet::new();
    };
    let mut by_txid: BTreeMap<Hash64, Option<Hash64>> = BTreeMap::new();
    for ((table, key), value) in &route.aux {
        if *table != PALW_FP_WINNING_CARRIER_TABLE_V1 || key.len() != 64 || value.len() != 64 {
            continue;
        }
        let Ok(key_bytes) = <[u8; 64]>::try_from(key.as_slice()) else { continue };
        let Ok(txid_bytes) = <[u8; 64]>::try_from(value.as_slice()) else { continue };
        let source = Hash64::from_bytes(key_bytes);
        let txid = Hash64::from_bytes(txid_bytes);
        by_txid.entry(txid).and_modify(|known| *known = None).or_insert(Some(source));
    }
    let mut edges = BTreeSet::new();
    for (consumer, txid) in spent {
        let Some(Some(source)) = by_txid.get(&txid).copied() else { continue };
        if source != consumer {
            edges.insert((consumer, source));
        }
    }
    edges
}

/// Funded-original inputs and ordinary free-prompt carrier inputs, in one list.
pub(super) fn carrier_spend_pairs_v1(extras: &crate::palw_state_v2::PalwTransitionExtrasV1) -> Vec<(Hash64, Hash64)> {
    extras
        .tir_funded_originals
        .iter()
        .flat_map(|(claim, pairing)| {
            pairing.carrier().inputs.iter().map(move |input| (*claim, input.previous_outpoint.transaction_id))
        })
        .chain(extras.carrier_input_spends.iter().flat_map(|(claim, txids)| txids.iter().map(move |txid| (*claim, *txid))))
        .collect()
}

/// Freeze the state at actual claim/certification or native row changes. A stuck A does not
/// acquire references to unrelated B frames merely by being in the same DAG.
pub(super) fn retain_changed_claim_context_v1(
    builder: &mut TransitionBuilder<'_>,
    parent: &PalwChainStateV2,
    ctx: &PalwBlockContextV2,
    accepted_objects: &[PalwConsensusObjectV2],
    block_work: PalwBlockWorkV3<'_>,
    merged_work: &[PalwMergedWorkV1<'_>],
    own_execution_key: Hash64,
) -> Result<(), PalwStateV2Error> {
    retain_changed_claim_context_inner_v1(
        builder,
        parent,
        ctx,
        accepted_objects,
        block_work,
        merged_work,
        own_execution_key,
        false,
        NativeRetentionModeV24::Legacy,
    )
    .map(|_| ())
}

/// Pure archive/journal fixture, with no checkpoint eligibility or activation.
/// The recorded policy remains the actual below-fence policy used by the fold.
#[cfg(test)]
pub(super) fn retain_changed_claim_projection_fixture_v1(
    builder: &mut TransitionBuilder<'_>,
    parent: &PalwChainStateV2,
    ctx: &PalwBlockContextV2,
    accepted_objects: &[PalwConsensusObjectV2],
) -> Result<(), PalwStateV2Error> {
    retain_changed_claim_context_inner_v1(
        builder,
        parent,
        ctx,
        accepted_objects,
        PalwBlockWorkV3::None,
        &[],
        Hash64::default(),
        true,
        NativeRetentionModeV24::Legacy,
    )
    .map(|_| ())
}

#[derive(Clone, Copy)]
enum NativeRetentionModeV24 {
    Legacy,
    Scoped,
}
/// The native main path retains this data-only boundary through all checkpoint
/// and GC writers, then verifies the complete suffix before publishing its delta.
pub(super) fn retain_changed_claim_context_scoped_v24(
    builder: &mut TransitionBuilder<'_>,
    parent: &PalwChainStateV2,
    ctx: &PalwBlockContextV2,
    accepted_objects: &[PalwConsensusObjectV2],
    block_work: PalwBlockWorkV3<'_>,
    merged_work: &[PalwMergedWorkV1<'_>],
    own_execution_key: Hash64,
) -> Result<
    Option<
        crate::palw_native_resource_v1::NativeRetainedV1<
            super::palw_claim_local_native_event_coverage_v21::NativeEventTailBoundaryV21,
        >,
    >,
    PalwStateV2Error,
> {
    retain_changed_claim_context_inner_v1(
        builder,
        parent,
        ctx,
        accepted_objects,
        block_work,
        merged_work,
        own_execution_key,
        false,
        NativeRetentionModeV24::Scoped,
    )
}

fn retain_changed_claim_context_inner_v1(
    builder: &mut TransitionBuilder<'_>,
    parent: &PalwChainStateV2,
    ctx: &PalwBlockContextV2,
    accepted_objects: &[PalwConsensusObjectV2],
    block_work: PalwBlockWorkV3<'_>,
    merged_work: &[PalwMergedWorkV1<'_>],
    own_execution_key: Hash64,
    projection_fixture: bool,
    mode: NativeRetentionModeV24,
) -> Result<
    Option<
        crate::palw_native_resource_v1::NativeRetainedV1<
            super::palw_claim_local_native_event_coverage_v21::NativeEventTailBoundaryV21,
        >,
    >,
    PalwStateV2Error,
> {
    if !projection_fixture && !builder.params.claim_local_settlement_active_at(ctx.daa_score) {
        return Ok(None);
    }
    // Capture even a native deletion-only block before the non-credit fast
    // path. Each row is fixed current admission provenance, not credit history.
    if !projection_fixture {
        super::palw_claim_local_kernel_current_birth_v9::capture_native_admissions_v9(builder, ctx)?;
    }
    let mining_coverage = if !projection_fixture && matches!(mode, NativeRetentionModeV24::Scoped) {
        Some(super::palw_claim_local_native_mining_birth_v20::capture_native_journal_with_coverage_v21(builder, ctx)?)
    } else {
        if !projection_fixture {
            super::palw_claim_local_native_mining_birth_v20::capture_native_journal_v20(builder, ctx)?;
        }
        None
    };
    // Non-credit owner updates are represented by the fresh same-point scoped
    // material inventory. They do not mint a new birth/approval history frame.
    if !projection_fixture
        && super::palw_claim_local_material_projection_v2::noncredit_only(builder, accepted_objects, block_work, merged_work)?
    {
        return Ok(None);
    }
    let (network_identity, fired_forks) = builder
        .extras
        .claim_local_rule_context_v1
        .ok_or_else(|| PalwStateV2Error::CarriageInconsistent("claim-local fold lacks original rule context".into()))?;
    if network_identity == [0; 32] || fired_forks == [0; 32] {
        return Err(PalwStateV2Error::CarriageInconsistent("claim-local rule context is zero".into()));
    }
    // Fully funded new-original-only blocks use their actual scoped sources.
    // The selector reads independently verified persisted receipts, not flags.
    // Mixed/legacy blocks retain the old F1 path; their separate global-copy
    // ownership remains explicit and is not charged to these source receipts.
    if !projection_fixture && funded_original_only_v17(builder, ctx, accepted_objects, block_work, merged_work)? {
        super::palw_kernel_route_fold_v1::ensure_route_header(builder, ctx)?;
        let inputs =
            native_claim_context_inputs_v17(builder, parent, ctx, accepted_objects, block_work, merged_work, own_execution_key)?;
        super::palw_claim_local_source_admission_v8::capture_admissions_v8(builder, parent, ctx, accepted_objects, &inputs)?;
        super::palw_claim_local_born_tir_source_v16::finalize_born_bundles_v16(builder, ctx, accepted_objects)?;
        for object in accepted_objects {
            let PalwConsensusObjectV2::FreePromptCommitted { claim, .. } = object else {
                unreachable!("scoped selector checked actual object kind")
            };
            if !super::palw_claim_local_born_tir_source_v16::has_verified_new_original_scope_in_fold_v25(builder, ctx, *claim)? {
                return Err(PalwStateV2Error::CarriageInconsistent(
                    "funded scoped path did not construct its verified native original archive".into(),
                ));
            }
        }
        return Ok(None); // Before native_changes/world copies or any F1 encoding.
    }
    // Own one actual input vector; capture sources before legacy world copies.
    // Dispatch is not permission: independent owners/capacity decide readiness.
    let mut scoped_inputs = None;
    if !projection_fixture && matches!(mode, NativeRetentionModeV24::Scoped) {
        let inputs =
            native_claim_context_inputs_v17(builder, parent, ctx, accepted_objects, block_work, merged_work, own_execution_key)?;
        let publication = prepare_native_scoped_publication_v21(builder, parent, ctx, accepted_objects, &inputs)?;
        let audit = super::palw_claim_local_native_event_coverage_v21::audit_captured_native_events_with_mining_v21(
            builder,
            ctx,
            mining_coverage.as_deref(),
        )?;
        let tail = if audit.first_gap_v21().is_none() { Some(audit.tail_boundary_v21()?) } else { None };
        drop(audit);
        if let Some(tail) = tail {
            super::palw_claim_local_native_event_coverage_v21::finalize_initial_mixed_sources_v21(builder, ctx, &publication)?;
            return Ok(Some(tail));
        }
        // Local context failure, NEVER a carrier-local drop or peer conviction.
        // Activation must preflight genuinely unsupported own scope before17.
        if publication.has_initial_originals_v24() {
            return Err(PalwStateV2Error::NativeContextUnavailable(
                crate::palw_source_bootstrap_v1::PalwSourceBootstrapContextFaultV1::HistoryUnavailable,
            ));
        }
        scoped_inputs = Some(inputs); // genuine unsupported history retains old F1
    }
    if builder.native_writers_tracking_v1() {
        let mut q = native_writers::NativeWriteQuoteV1::new("claim-local changed claims");
        q.member::<Hash64, ()>(builder.entries.len() as u64)?;
        q.exact::<PalwClaimLocalNativeChangeV1>(builder.entries.len() as u64)?;
        for entry in &builder.entries {
            match entry {
                PalwDeltaEntryV2::KernelRouteRow { table, key, old, new } if *table != PALW_CLAIM_LOCAL_TABLE_V1 => {
                    q.copy(key)?.copy(old)?.copy(new)?;
                }
                PalwDeltaEntryV2::ExecV2Row { key, old, new, .. } => {
                    q.copy(key)?.copy(old)?.copy(new)?;
                }
                PalwDeltaEntryV2::Claim { old, new, .. } => {
                    q.exact::<u8>(64)?;
                    if let Some(row) = old {
                        q.encoded(row)?;
                    }
                    if let Some(row) = new {
                        q.encoded(row)?;
                    }
                }
                _ => {}
            }
        }
        builder.admit_native_write_v1(q)?;
    }
    let mut claims = BTreeSet::new();
    let mut native_changes = Vec::with_capacity(builder.entries.len());
    for entry in &builder.entries {
        let native = match entry {
            PalwDeltaEntryV2::KernelRouteRow { table, key, old, new } if *table != PALW_CLAIM_LOCAL_TABLE_V1 => {
                Some((PalwClaimLocalRowOwnerV1::KernelRoute, table, key, old, new))
            }
            PalwDeltaEntryV2::ExecV2Row { table, key, old, new } => Some((PalwClaimLocalRowOwnerV1::ExecV2, table, key, old, new)),
            _ => None,
        };
        if let Some((owner, table, key, old, new)) = native {
            native_changes.push(PalwClaimLocalNativeChangeV1 {
                owner,
                table: *table,
                key: key.clone(),
                old: old.clone(),
                new: new.clone(),
            });
        }
        match entry {
            PalwDeltaEntryV2::Claim { key, old, new } => {
                native_changes.push(PalwClaimLocalNativeChangeV1 {
                    owner: PalwClaimLocalRowOwnerV1::ChainClaims,
                    table: 0,
                    key: key.as_byte_slice().to_vec(),
                    old: old.as_ref().map(|r| borsh::to_vec(r).expect("canonical original claim")),
                    new: new.as_ref().map(|r| borsh::to_vec(r).expect("canonical current claim")),
                });
                claims.insert(*key);
            }
            PalwDeltaEntryV2::KernelRouteRow { table, key, old, new }
                if *table == crate::palw_security_certification_v1::PALW_SECURITY_CERTIFICATION_TABLE_V1
                    && key.len() == 65
                    && matches!(
                        key[0],
                        crate::palw_security_certification_v1::CERTIFICATION_CLAIM_PREFIX_V1
                            | crate::palw_security_certification_v1::CERTIFICATION_NATIVE_MINING_PREFIX_V39
                    ) =>
            {
                // The canonical row starts with its claim id. Read that fixed
                // identity without allocating the embedded draws/attestations.
                let bytes = new
                    .as_deref()
                    .or(old.as_deref())
                    .ok_or_else(|| PalwStateV2Error::CarriageInconsistent("empty certification journal".into()))?;
                let id: [u8; 64] = bytes
                    .get(..64)
                    .ok_or_else(|| PalwStateV2Error::CarriageInconsistent("short certification journal".into()))?
                    .try_into()
                    .expect("exact fixed claim id");
                claims.insert(Hash64::from_bytes(id));
            }
            _ => {}
        }
    }
    if claims.is_empty() && native_changes.is_empty() {
        return Ok(None);
    }
    super::palw_kernel_route_fold_v1::ensure_route_header(builder, ctx)?;
    if !projection_fixture && matches!(mode, NativeRetentionModeV24::Legacy) {
        super::palw_claim_local_exec_archive_v10::capture_executed_exec_v10(builder, parent, ctx)?;
    }
    let key = palw_claim_liability_frame_key_v1(ctx.block);
    if builder.state.kernel_route.as_ref().is_some_and(|route| route.aux.contains_key(&(PALW_CLAIM_LOCAL_TABLE_V1, key.to_vec()))) {
        return Err(PalwStateV2Error::CarriageInconsistent("liability frame would replace an original context".into()));
    }
    if builder.native_writers_tracking_v1() {
        let mut q = native_writers::NativeWriteQuoteV1::new("claim-local original context copies");
        q.copy(parent)?.copy(&builder.state)?.copy(&builder.extras.exec_v2_origin_headers)?;
        q.exact::<PalwConsensusObjectV2>(accepted_objects.len() as u64)?;
        for object in accepted_objects {
            q.copy(object)?;
        }
        q.exact::<crate::palw_detection_policy_v1::PalwDetectionPolicyV1>(
            PalwClaimLocalPolicyContextV1::detection_at_v1(builder.params, ctx.daa_score).len() as u64,
        )?;
        builder.admit_native_write_v1(q)?;
    }
    // Preserve the actual presented attempt/spend metadata and transaction-local
    // original envelopes. A post-pruning verifier must not reconstruct these
    // from a digest or from a current/replaced service contract.
    let inputs = match scoped_inputs {
        Some(inputs) => inputs,
        None => native_claim_context_inputs_v17(builder, parent, ctx, accepted_objects, block_work, merged_work, own_execution_key)?,
    };
    if !projection_fixture && matches!(mode, NativeRetentionModeV24::Legacy) {
        super::palw_claim_local_source_admission_v8::capture_admissions_v8(builder, parent, ctx, accepted_objects, &inputs)?;
        super::palw_claim_local_legacy_terminal_v18::capture_live_current_v18(builder, ctx)?;
        super::palw_claim_local_legacy_terminal_v18::capture_selected_cert_progress_v18(builder, ctx)?;
    }
    if !projection_fixture && matches!(mode, NativeRetentionModeV24::Legacy) && original_archive_capture_requested_v17(builder.extras)
    {
        super::palw_claim_local_born_tir_source_v16::finalize_born_bundles_v16(builder, ctx, accepted_objects)?;
    }
    let prior = PalwStateCarriageV2::from_state_for_claim_local_frame_v1(parent);
    let mut current = PalwStateCarriageV2::from_state_for_claim_local_frame_v1(&builder.state);
    current.last_point = Some(*ctx);
    if builder.native_writers_tracking_v1() {
        let mut q = native_writers::NativeWriteQuoteV1::new("claim-local original context encoding");
        q.encoded(&prior)?.encoded(&current)?;
        builder.admit_native_write_v1(q)?;
    }
    let frame = PalwClaimLiabilityFrameV1 {
        point: *ctx,
        network_identity,
        fired_forks,
        policy: PalwClaimLocalPolicyContextV1::from_rules_v1(builder.params, ctx.daa_score),
        claims,
        native_changes,
        prior_state: borsh::to_vec(&prior).expect("canonical original carriage"),
        current_state: borsh::to_vec(&current).expect("canonical current carriage"),
        accepted_objects: accepted_objects.to_vec(),
        origin_headers: builder.extras.exec_v2_origin_headers.clone(),
        inputs,
    };
    builder.write_kernel_row_encoded_v1(PALW_CLAIM_LOCAL_TABLE_V1, &key, Some(&frame))?;
    Ok(None)
}

fn original_archive_capture_requested_v17(extras: &PalwTransitionExtrasV1) -> bool {
    if super::palw_tir_public_fold_v1::original_archive_reservation_proposal_v1::original_archive_profiles_selected_v2(extras) {
        return true;
    }
    #[cfg(test)]
    {
        extras.original_archive_data_observer_v16
    }
    #[cfg(not(test))]
    {
        false
    }
}
/// No unaccepted expected DTO or observer flag selects this path. Every actual
/// fresh Claim and outer FP object must have its matching native funded receipt.
/// Older claim changes/work/other object kinds preserve independent legacy F1.
fn funded_original_only_v17(
    b: &TransitionBuilder<'_>,
    point: &PalwBlockContextV2,
    objects: &[PalwConsensusObjectV2],
    work: PalwBlockWorkV3<'_>,
    merged: &[PalwMergedWorkV1<'_>],
) -> Result<bool, PalwStateV2Error> {
    if !matches!(work, PalwBlockWorkV3::None) || !merged.is_empty() || objects.is_empty() {
        return Ok(false);
    }
    if !super::palw_tir_public_fold_v1::original_archive_reservation_proposal_v1::original_archive_profiles_selected_v1(b.extras) {
        return Ok(false);
    }
    let mut fresh = false;
    for (ordinal, entry) in b.entries.iter().enumerate() {
        match entry {
            PalwDeltaEntryV2::KernelRouteRow { table, .. } if *table != PALW_CLAIM_LOCAL_TABLE_V1 => {
                if !b.claim_local_object_spans.iter().any(|span| span.first <= ordinal && ordinal < span.end) {
                    return Ok(false);
                }
            }
            PalwDeltaEntryV2::Claim { key, old: None, new: Some(c) } => {
                if (c.accepted_block, c.accepted_daa, c.accepted_blue_score) != (point.block, point.daa_score, point.blue_score) {
                    return Ok(false);
                }
                let Some(receipt)=super::palw_tir_public_fold_v1::original_archive_reservation_proposal_v1::verified_original_archive_reservation_v1(&b.state,*key,*point)? else{return Ok(false)};
                if receipt.profile_kind_v2() != crate::palw_court_original_escrow_v1::OriginalArchiveProfileKindV2::MaterialV1 {
                    return Ok(false);
                }
                fresh = true;
            }
            PalwDeltaEntryV2::Claim { .. } | PalwDeltaEntryV2::ExecV2Row { .. } => return Ok(false),
            _ => {}
        }
    }
    if !fresh {
        return Ok(false);
    }
    for object in objects {
        let PalwConsensusObjectV2::FreePromptCommitted { claim, .. } = object else { return Ok(false) };
        let Some(receipt) =
            super::palw_tir_public_fold_v1::original_archive_reservation_proposal_v1::verified_original_archive_reservation_v1(
                &b.state, *claim, *point,
            )?
        else {
            return Ok(false);
        };
        if receipt.profile_kind_v2() != crate::palw_court_original_escrow_v1::OriginalArchiveProfileKindV2::MaterialV1 {
            return Ok(false);
        }
    }
    Ok(true)
}
/// Shared unchanged literal builder for legacy and funded scoped routes. It
/// makes no prior/current world carriage and retains native preallocation gates.
fn native_claim_context_inputs_v17(
    builder: &mut TransitionBuilder<'_>,
    parent: &PalwChainStateV2,
    ctx: &PalwBlockContextV2,
    accepted_objects: &[PalwConsensusObjectV2],
    block_work: PalwBlockWorkV3<'_>,
    merged_work: &[PalwMergedWorkV1<'_>],
    own_execution_key: Hash64,
) -> Result<Vec<PalwClaimLocalInputV1>, PalwStateV2Error> {
    let consumed = if original_archive_capture_requested_v17(builder.extras) {
        consumed_carrier_edges_targeted_v17(builder, parent)?
    } else {
        consumed_carrier_edges_v1(parent, carrier_spend_pairs_v1(builder.extras))
    };
    let fp_count =
        accepted_objects.iter().filter(|object| matches!(object, PalwConsensusObjectV2::FreePromptCommitted { .. })).count();
    let local_count = builder
        .extras
        .history_custody_pledges
        .len()
        .checked_add(builder.extras.public_original_bindings.len())
        .and_then(|n| n.checked_add(builder.extras.producer_custody_delegations.len()))
        .and_then(|n| n.checked_add(builder.extras.tir_public_originals.len()))
        .and_then(|n| n.checked_add(merged_work.len()))
        .and_then(|n| n.checked_add(1))
        .and_then(|n| n.checked_add(fp_count))
        .and_then(|n| n.checked_add(builder.extras.prefix_dependencies_v1.len()))
        .and_then(|n| n.checked_add(consumed.len()))
        .ok_or(PalwStateV2Error::Overflow("liability inputs"))?;
    if builder.native_writers_tracking_v1() {
        let mut q = native_writers::NativeWriteQuoteV1::new("claim-local input carriage copies");
        q.exact::<PalwClaimLocalInputV1>(local_count as u64)?;
        q.copy(&builder.extras.history_custody_pledges)?
            .copy(&builder.extras.public_original_bindings)?
            .copy(&builder.extras.producer_custody_delegations)?
            .copy(&builder.extras.tir_public_originals)?
            .copy(&builder.extras.prefix_dependencies_v1)?;
        q.exact::<crate::palw_claim_local_v1::prefix_v1::PalwPrefixDependencyEnvelopeV1>(
            builder.extras.prefix_dependencies_v1.len() as u64
        )?;
        for object in accepted_objects {
            if let PalwConsensusObjectV2::FreePromptCommitted { public_job, eval, .. } = object {
                q.copy(public_job)?;
                if let Some(eval) = eval {
                    q.exact::<(crate::palw_improve_eval_v1::PalwEvalJobV1, crate::palw_improve_eval_v1::PalwEvalClaimTailV1)>(1)?;
                    q.copy(&eval.job)?.copy(&eval.tail)?;
                }
            }
        }
        for work in std::iter::once(block_work).chain(merged_work.iter().map(|m| m.work)) {
            match work {
                PalwBlockWorkV3::Attempt(e) => {
                    q.exact::<PalwAttemptEnvelopeV2>(1)?.copy(e)?;
                }
                PalwBlockWorkV3::ReceiptSpend(s) => {
                    q.exact::<PalwReceiptSpendUnsignedV3>(1)?.copy(s)?;
                }
                PalwBlockWorkV3::None => {}
            }
        }
        builder.admit_native_write_v1(q)?;
    }
    let mut inputs = Vec::with_capacity(local_count);
    for (block, execution_key, work) in std::iter::once((ctx.block, own_execution_key, block_work))
        .chain(merged_work.iter().map(|m| (m.carrying_block, m.execution_key, m.work)))
    {
        match work {
            PalwBlockWorkV3::Attempt(e) => {
                inputs.push(PalwClaimLocalInputV1::Attempt { block, execution_key, envelope: Box::new(e.clone()) })
            }
            PalwBlockWorkV3::ReceiptSpend(s) => inputs.push(PalwClaimLocalInputV1::ReceiptSpend { block, spend: Box::new(s.clone()) }),
            PalwBlockWorkV3::None => {}
        }
    }
    for (claim, envelope) in &builder.extras.prefix_dependencies_v1 {
        if accepted_objects.iter().any(|o| matches!(o, PalwConsensusObjectV2::FreePromptCommitted { claim: id, .. } if id == claim)) {
            inputs.push(PalwClaimLocalInputV1::PrefixDependency { claim: *claim, envelope: Box::new(envelope.clone()) });
        }
    }
    for (claim, pledge) in &builder.extras.history_custody_pledges {
        inputs.push(PalwClaimLocalInputV1::History { claim: *claim, pledge: Box::new(pledge.clone()) });
    }
    for (claim, header) in &builder.extras.public_original_bindings {
        inputs.push(PalwClaimLocalInputV1::PublicBinding { claim: *claim, header: Box::new(header.clone()) });
    }
    for (claim, delegation) in &builder.extras.producer_custody_delegations {
        inputs.push(PalwClaimLocalInputV1::CustodyDelegation { claim: *claim, delegation: Box::new(delegation.clone()) });
    }
    for (claim, envelope) in &builder.extras.tir_public_originals {
        inputs.push(PalwClaimLocalInputV1::TirOriginal { claim: *claim, envelope: Box::new(envelope.clone()) });
    }
    for (consumer, source) in &consumed {
        inputs.push(PalwClaimLocalInputV1::ConsumedCarrier { consumer: *consumer, source: *source });
    }
    for object in accepted_objects {
        if let PalwConsensusObjectV2::FreePromptCommitted { claim, job_pin, public_job, eval, .. } = object {
            inputs.push(PalwClaimLocalInputV1::FreePromptNative {
                claim: *claim,
                job_pin: *job_pin,
                public_job: public_job.clone(),
                eval: eval.as_ref().map(|data| Box::new((data.job.clone(), data.tail.clone()))),
            });
        }
    }
    Ok(inputs)
}

/// Actual input-edge capacity is admitted before building only this block's
/// spend list/result set. No parent winner map or unrelated source is cloned.
fn consumed_carrier_edges_targeted_v17(
    b: &mut TransitionBuilder<'_>,
    parent: &PalwChainStateV2,
) -> Result<BTreeSet<(Hash64, Hash64)>, PalwStateV2Error> {
    let count = b
        .extras
        .tir_funded_originals
        .values()
        .try_fold(0usize, |n, p| n.checked_add(p.carrier().inputs.len()))
        .and_then(|n| b.extras.carrier_input_spends.values().try_fold(n, |n, txids| n.checked_add(txids.len())))
        .ok_or(PalwStateV2Error::Overflow("targeted carrier input count"))?;
    let mut q = native_writers::NativeWriteQuoteV1::new("targeted current carrier input projection");
    q.exact::<(Hash64, Hash64)>(count as u64)?;
    q.exact::<crate::palw_native_resource_v1::BTreeNodeEnvelopeV1<(Hash64, Hash64), ()>>(count as u64)?;
    b.admit_native_write_v1(q)?;
    let mut spent = Vec::with_capacity(count);
    for (claim, pair) in &b.extras.tir_funded_originals {
        for input in &pair.carrier().inputs {
            spent.push((*claim, input.previous_outpoint.transaction_id));
        }
    }
    for (claim, txids) in &b.extras.carrier_input_spends {
        for txid in txids {
            spent.push((*claim, *txid));
        }
    }
    let mut edges = BTreeSet::new();
    for (consumer, txid) in spent {
        if let Some(source) = super::palw_claim_local_carrier_lineage_v9::current_winner_claim_targeted_v17(parent, &b.state, txid)?
            && source != consumer
        {
            edges.insert((consumer, source));
        }
    }
    Ok(edges)
}

/// Same exact legacy admission validation for an authenticated immutable F1
/// artifact retained under a native paid ancestor descriptor. No frame/world is
/// synthesized, and no current params are recorded as historical provenance.
pub(super) fn admission_from_native_retained_frame_v19(
    frame: crate::palw_native_resource_v1::NativeRetainedV1<PalwClaimLiabilityFrameV1>,
    accepting: &PalwBlockContextV2,
    id: Hash64,
) -> Result<Option<PalwClaimLocalAdmissionContextV1>, PalwStateV2Error> {
    if frame.point != *accepting {
        return Err(PalwStateV2Error::CarriageInconsistent("frozen native F1 exact accepting point differs".into()));
    }
    let original = frame
        .with_context_carriages_v1(|prior, current| {
            // The producer's changed-claim list alone is never birth authority.
            if !frame.claims.contains(&id) {
                return Ok(None);
            }
            let Some(claim) = current.claims.get(&id) else { return Ok(None) };
            // A retired FP identity can be admitted again in the same
            // transition that removes its old record. Scope the birth by
            // its accepting point, not global absence of the raw claim id.
            if prior
                .claims
                .get(&id)
                .is_some_and(|old| old.accepted_daa == claim.accepted_daa && old.accepted_blue_score == claim.accepted_blue_score)
            {
                return Ok(None);
            }
            if claim.accepted_daa != accepting.daa_score || claim.accepted_blue_score != accepting.blue_score {
                return Err("claim birth differs from the shared accepting point".into());
            }
            let has_input =
                match claim.source {
                    PalwClaimSourceV2::Attempt => {
                        frame.inputs.iter().try_fold(false, |found, input| -> Result<bool, PalwClaimCheckpointErrorV1> {
                            if found {
                                return Ok(true);
                            }
                            let PalwClaimLocalInputV1::Attempt { block, envelope, .. } = input else { return Ok(false) };
                            let a = &envelope.attempt;
                            if *block != claim.accepted_block
                                || a.class_id != claim.class_id
                                || a.executor_bond != claim.bond.0
                                || a.trace_root != claim.trace_root
                                || a.output_root != claim.output_root
                                || a.execution_root != claim.execution_root
                            {
                                return Ok(false);
                            }
                            use crate::palw_native_resource_v1::{NativeCoverageV1, TirNativeRequestsV1, reserve_native_requests_v1};
                            let len = borsh::object_length(a).map_err(|why| {
                                PalwClaimCheckpointErrorV1::Resource(NativeCoverageV1::unsupported::<PalwAttemptEnvelopeV2>(
                                    "admission identity length",
                                    &why.to_string(),
                                ))
                            })? as u64;
                            let mut q = TirNativeRequestsV1::new();
                            q.vec_v1::<u8>(len.max(1024)).map_err(PalwClaimCheckpointErrorV1::Resource)?;
                            let _hash_workspace =
                                reserve_native_requests_v1(q.bytes_v1()).map_err(PalwClaimCheckpointErrorV1::Resource)?;
                            Ok(crate::palw_attempt_v2::attempt_id_v2(a) == id)
                        })?
                    }
                    PalwClaimSourceV2::FreePrompt { .. } => frame.inputs.iter().any(
                        |input| matches!(input, PalwClaimLocalInputV1::FreePromptNative { claim: native_id, .. } if *native_id == id),
                    ) && frame.accepted_objects.iter().any(|object| {
                        matches!(object,
                    PalwConsensusObjectV2::FreePromptCommitted {
                        claim: object_id, class_id, bond, trace_root, output_root, execution_root, ..
                    } if *object_id == id && *class_id == claim.class_id && *bond == claim.bond
                        && *trace_root == claim.trace_root && *output_root == claim.output_root
                        && *execution_root == claim.execution_root)
                    }),
                };
            if !has_input {
                return Err("claim admission lost its presented original input".into());
            }
            use crate::palw_native_resource_v1::{NativeRetainedV1, native_value_requests_v1, reserve_native_requests_v1};
            let demand = native_value_requests_v1(claim).map_err(PalwClaimCheckpointErrorV1::Resource)?;
            let lease = reserve_native_requests_v1(demand).map_err(PalwClaimCheckpointErrorV1::Resource)?;
            Ok(Some(NativeRetainedV1::from_owner_reserved_v1(claim.clone(), lease)))
        })
        .map_err(|why| match why {
            PalwClaimCheckpointErrorV1::Resource(why) => PalwStateV2Error::NativeResource(why),
            PalwClaimCheckpointErrorV1::Semantic(why) => PalwStateV2Error::CarriageInconsistent(why.into()),
        })?;
    Ok(original.map(|claim| PalwClaimLocalAdmissionContextV1 { id, frame, claim }))
}

/// Actual full-fold preparation, staged separately from route activation.
/// Caller must have completed the ONE whole-batch census/baseline observation
/// and must retain the actual input builder's ticket. This function publishes
/// no legacy F1 and makes no claim that unrelated historical owners are ready.
/// Initial OAR2 mark is intentionally deferred to the final ownership seal.
fn prepare_native_scoped_publication_v21(
    b: &mut TransitionBuilder<'_>,
    parent: &PalwChainStateV2,
    ctx: &PalwBlockContextV2,
    objects: &[PalwConsensusObjectV2],
    inputs: &[PalwClaimLocalInputV1],
) -> Result<
    crate::palw_native_resource_v1::NativeRetainedV1<
        super::palw_claim_local_native_event_coverage_v21::NativeMixedInitialPublicationV21,
    >,
    PalwStateV2Error,
> {
    if b.claim_local_lock_point_v6 != Some(*ctx) {
        return Err(PalwStateV2Error::CarriageInconsistent("scoped native publication lacks actual executing point".into()));
    }
    super::palw_kernel_route_fold_v1::ensure_route_header(b, ctx)?;
    super::palw_claim_local_exec_archive_v10::capture_executed_exec_v10(b, parent, ctx)?;
    super::palw_claim_local_source_admission_v8::capture_admissions_v8(b, parent, ctx, objects, inputs)?;
    if original_archive_capture_requested_v17(b.extras) {
        super::palw_claim_local_born_tir_source_v16::finalize_born_bundles_v16(b, ctx, objects)?;
    }
    let mixed = super::palw_claim_local_native_event_coverage_v21::capture_initial_mixed_sources_v21(b, ctx, objects)?;
    super::palw_claim_local_legacy_terminal_v18::capture_live_current_v18(b, ctx)?;
    super::palw_claim_local_legacy_terminal_v18::capture_selected_cert_progress_v18(b, ctx)?;
    Ok(mixed)
}
