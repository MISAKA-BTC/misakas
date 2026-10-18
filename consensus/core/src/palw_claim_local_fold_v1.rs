//! Historical context comes from the executing fold, never a producer list.
//! Credit/frontier integration is deliberately separate from this journal writer.
use super::*;
use crate::palw_claim_local_v1::*;

impl PalwChainStateV2 {
    pub(super) fn assert_claim_local_context_v1(&self, params: &PalwStateParamsV2) -> Result<(), PalwStateV2Error> {
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

/// Freeze the state at actual claim/certification changes. A stuck A does not
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
    if !builder.params.claim_local_settlement_active_at(ctx.daa_score) {
        return Ok(());
    }
    let (network_identity, fired_forks) = builder
        .extras
        .claim_local_rule_context_v1
        .ok_or_else(|| PalwStateV2Error::CarriageInconsistent("claim-local fold lacks original rule context".into()))?;
    if network_identity == [0; 32] || fired_forks == [0; 32] {
        return Err(PalwStateV2Error::CarriageInconsistent("claim-local rule context is zero".into()));
    }
    if builder.native_writers_tracking_v1() {
        let mut q = native_writers::NativeWriteQuoteV1::new("claim-local changed claims");
        q.member::<Hash64, ()>(builder.entries.len() as u64)?;
        builder.admit_native_write_v1(q)?;
    }
    let mut claims = BTreeSet::new();
    for entry in &builder.entries {
        match entry {
            PalwDeltaEntryV2::Claim { key, .. } => {
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
    if claims.is_empty() {
        return Ok(());
    }
    super::palw_kernel_route_fold_v1::ensure_route_header(builder, ctx)?;
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
    // Preserve the actual presented signed attempt/spend and transaction-local
    // original envelopes. A post-pruning verifier must not reconstruct these
    // from a digest or from a current/replaced service contract.
    let local_count = builder
        .extras
        .history_custody_pledges
        .len()
        .checked_add(builder.extras.public_original_bindings.len())
        .and_then(|n| n.checked_add(builder.extras.producer_custody_delegations.len()))
        .and_then(|n| n.checked_add(builder.extras.tir_public_originals.len()))
        .and_then(|n| n.checked_add(merged_work.len()))
        .and_then(|n| n.checked_add(1))
        .ok_or(PalwStateV2Error::Overflow("liability inputs"))?;
    if builder.native_writers_tracking_v1() {
        let mut q = native_writers::NativeWriteQuoteV1::new("claim-local input carriage copies");
        q.exact::<PalwClaimLocalInputV1>(local_count as u64)?;
        q.copy(&builder.extras.history_custody_pledges)?
            .copy(&builder.extras.public_original_bindings)?
            .copy(&builder.extras.producer_custody_delegations)?
            .copy(&builder.extras.tir_public_originals)?;
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
        prior_state: borsh::to_vec(&prior).expect("canonical original carriage"),
        current_state: borsh::to_vec(&current).expect("canonical current carriage"),
        accepted_objects: accepted_objects.to_vec(),
        origin_headers: builder.extras.exec_v2_origin_headers.clone(),
        inputs,
    };
    builder.write_kernel_row_encoded_v1(PALW_CLAIM_LOCAL_TABLE_V1, &key, Some(&frame))
}
