//! Historical context comes from the executing fold, never a producer list.
//! Credit/frontier integration is deliberately separate from this journal writer.
use super::*;
use crate::palw_claim_local_v1::*;

impl PalwChainStateV2 {
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
        let original =
            frame
                .with_context_carriages_v1(|prior, current| {
                    // The producer's changed-claim list alone is never birth authority.
                    if !frame.claims.contains(&id) {
                        return Ok(None);
                    }
                    let Some(claim) = current.claims.get(&id) else { return Ok(None) };
                    // A retired FP identity can be admitted again in the same
                    // transition that removes its old record. Scope the birth by
                    // its accepting point, not global absence of the raw claim id.
                    if prior.claims.get(&id).is_some_and(|old| {
                        old.accepted_daa == claim.accepted_daa && old.accepted_blue_score == claim.accepted_blue_score
                    }) {
                        return Ok(None);
                    }
                    if claim.accepted_daa != accepting.daa_score || claim.accepted_blue_score != accepting.blue_score {
                        return Err("claim birth differs from the shared accepting point".into());
                    }
                    let has_input = match claim.source {
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
    // Preserve the actual presented attempt/spend metadata and transaction-local
    // original envelopes. A post-pruning verifier must not reconstruct these
    // from a digest or from a current/replaced service contract.
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
        .ok_or(PalwStateV2Error::Overflow("liability inputs"))?;
    if builder.native_writers_tracking_v1() {
        let mut q = native_writers::NativeWriteQuoteV1::new("claim-local input carriage copies");
        q.exact::<PalwClaimLocalInputV1>(local_count as u64)?;
        q.copy(&builder.extras.history_custody_pledges)?
            .copy(&builder.extras.public_original_bindings)?
            .copy(&builder.extras.producer_custody_delegations)?
            .copy(&builder.extras.tir_public_originals)?;
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
