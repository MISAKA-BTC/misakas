//! Aux137 (renumbered from 134) uses one pre-reserved cell per contract through Active -> Terminal.
//! No Release can fail because a receipt/index table needs a new slot.
use super::*;
use crate::palw_material_release_recovery_v2::*;
fn refused(s: impl Into<String>) -> PalwStateV2Error {
    PalwStateV2Error::ExecV2Refused(s.into())
}
fn write(b: &mut TransitionBuilder<'_>, root: Hash64, row: Option<&PalwMaterialRecoveryRowV2>) -> Result<(), PalwStateV2Error> {
    b.write_kernel_row(PALW_MATERIAL_RELEASE_RECOVERY_TABLE_V2, root.as_byte_slice().to_vec(), row.map(|r| borsh::to_vec(r).unwrap()))?;
    Ok(())
}
fn provider_keys(signatures: &[PalwMaterialRecoverySignatureV2]) -> Vec<PalwBondKeyV2> {
    signatures.iter().map(|s| s.provider).collect()
}

fn plan_streams(
    b: &TransitionBuilder<'_>,
    p: &PalwMaterialReleaseRecoveryPolicyV2,
    ctx: &PalwBlockContextV2,
    contract: Hash64,
    providers: &[PalwBondKeyV2],
) -> Result<Vec<PalwMaterialProviderAckCheckpointV2>, PalwStateV2Error> {
    let mut new_count = 0usize;
    let mut planned = Vec::new();
    for provider in providers {
        let mut checkpoint = match b.state.material_provider_ack_checkpoint_v2(provider) {
            Some(checkpoint) => checkpoint,
            None => {
                new_count += 1;
                PalwMaterialProviderAckCheckpointV2 {
                    version: 2,
                    provider: *provider,
                    stream: Hash64::from_bytes(misaka_palw_kernel::hash::object_id(
                        b"MISAKA/PALW/MATERIAL-PROVIDER-ACK-STREAM/V2",
                        &(contract, provider),
                    )),
                    created_daa: ctx.daa_score,
                    sequence: 0,
                    reserved_acks: 0,
                    reserved_answer_events: 0,
                    last_receipt: Hash64::default(),
                }
            }
        };
        checkpoint.reserved_acks = checkpoint
            .reserved_acks
            .checked_add(1)
            .filter(|n| checkpoint.sequence.checked_add(*n).and_then(|n| n.checked_add(checkpoint.reserved_answer_events)).is_some())
            .ok_or_else(|| refused("provider ACK sequence capacity exhausted before admission"))?;
        planned.push(checkpoint);
    }
    let used = b
        .state
        .kernel_route()
        .map(|r| {
            r.aux
                .keys()
                .filter(|(t, k)| *t == PALW_MATERIAL_RELEASE_RECOVERY_TABLE_V2 && k.len() == 69 && k.first() == Some(&0xA1))
                .count()
        })
        .unwrap_or(0);
    if used.checked_add(new_count).is_none_or(|n| n > p.max_provider_streams as usize) {
        return Err(refused("provider ACK stream slots unavailable before admission"));
    }
    Ok(planned)
}
fn write_stream(b: &mut TransitionBuilder<'_>, s: &PalwMaterialProviderAckCheckpointV2) -> Result<(), PalwStateV2Error> {
    b.write_kernel_row(
        PALW_MATERIAL_RELEASE_RECOVERY_TABLE_V2,
        material_provider_stream_key_v2(&s.provider),
        Some(borsh::to_vec(s).unwrap()),
    )?;
    Ok(())
}

pub(super) fn action(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    a: &PalwMaterialRecoveryActionV2,
    signer: &PalwBondKeyV2,
) -> Result<(), PalwStateV2Error> {
    let p = b
        .params
        .material_release_recovery_at_v2(ctx.daa_score)
        .ok_or_else(|| refused("release recovery own V2 gate is dormant"))?
        .clone();
    p.validate().map_err(refused)?;
    if let PalwMaterialRecoveryActionV2::ReplaceBeforeV4 { valid_until_daa, .. } = a {
        let monetary =
            b.params.monetary_material_at_v1(ctx.daa_score).ok_or_else(|| refused("bounded replacement monetary gate dormant"))?;
        if *valid_until_daa < ctx.daa_score
            || ctx.daa_score.checked_add(monetary.max_enrollment_daa).is_none_or(|max| *valid_until_daa > max)
        {
            return Err(refused("bounded replacement signed admission deadline is expired or outside policy"));
        }
    }
    match a {
        PalwMaterialRecoveryActionV2::AnswerAndCheckpoint { answer, stream, previous, sequence, challenge_deadline_daa } => {
            let mut checkpoint =
                b.state.material_provider_ack_checkpoint_v2(signer).ok_or_else(|| refused("answer checkpoint stream absent"))?;
            if answer.provider() != *signer
                || checkpoint.stream != *stream
                || checkpoint.last_receipt != *previous
                || checkpoint.sequence.checked_add(1) != Some(*sequence)
                || checkpoint.reserved_answer_events == 0
            {
                return Err(refused("answer stream/sequence/previous/owner is replayed or substituted"));
            }
            let row = b.state.monetary_material_row_v1(answer.root()).ok_or_else(|| refused("answer recovery root absent"))?;
            let bound = row.challenges.iter().any(|c| {
                !c.answered
                    && c.deadline_daa == *challenge_deadline_daa
                    && match answer.as_ref() {
                        PalwMaterialRecoveryAnswerV4::Held { dependency, provider, missing, .. } => {
                            c.provider == *provider && c.dependency == *dependency && c.native_missing == Some(*missing)
                        }
                        PalwMaterialRecoveryAnswerV4::Kernel { dependency, provider, stage, position, .. } => {
                            c.provider == *provider
                                && c.dependency == *dependency
                                && c.stage == *stage
                                && c.position == *position
                                && c.native_missing.is_none()
                                && c.segment_part.is_none()
                        }
                        PalwMaterialRecoveryAnswerV4::Segmented { dependency, provider, position, part, .. } => {
                            c.provider == *provider
                                && c.dependency == *dependency
                                && c.position == *position
                                && c.segment_part == Some(*part)
                        }
                    }
            });
            if !bound {
                return Err(refused("answer substitutes its exact challenge/deadline"));
            }
            let event = Hash64::from_bytes(misaka_palw_kernel::hash::object_id(b"MISAKA/PALW/MATERIAL-ANSWER-EVENT/V4", &a.payload()));
            let ordinary = answer.as_ref().clone().into_ordinary();
            super::palw_monetary_material_fold_v1::action_with_recovery_answer_v4(b, ctx, &ordinary, signer)?;
            checkpoint.sequence = *sequence;
            checkpoint.last_receipt = event;
            checkpoint.reserved_answer_events -= 1;
            write_stream(b, &checkpoint)?;
        }
        PalwMaterialRecoveryActionV2::FundedEnroll { monetary, terms, providers, .. } => {
            if *signer != terms.sponsor
                || terms.sponsor != monetary.terms.sponsor
                || terms.root != monetary.terms.root
                || terms.initial_consent != monetary.id()
                || b.state.monetary_material_row_v1(terms.root).is_some()
                || b.state.material_recovery_row_v2(terms.root).is_some()
                || b.state.claim(&terms.root).is_some()
                || b.state.kernel_route().and_then(|r| r.kernel_claim_row_v1(&terms.root)).is_some()
                || b.state.exec_v2_root_v1(&terms.root).is_some()
                || provider_keys(providers) != monetary.offers.iter().map(|o| o.provider).collect::<Vec<_>>()
            {
                return Err(refused("funded recovery substitutes fresh root/consent/sponsor/providers"));
            }
            let row = PalwMaterialRecoveryRowV2::Active(PalwMaterialRecoveryContractV2 {
                terms: terms.clone(),
                enrolled_daa: ctx.daa_score,
                generations: vec![PalwMaterialRecoveryGenerationV2 { consent: monetary.id(), providers: provider_keys(providers) }],
            });
            row.validate(&p).map_err(refused)?;
            let count = b
                .state
                .kernel_route()
                .map(|r| r.aux.keys().filter(|(t, k)| *t == PALW_MATERIAL_RELEASE_RECOVERY_TABLE_V2 && k.len() == 64).count())
                .unwrap_or(0);
            if count >= p.max_reserved_contracts as usize
                || (count as u64 + 1) * u64::from(p.reserved_cells()) > u64::from(p.max_reserved_history_cells)
            {
                return Err(refused("funded recovery reservation unavailable before financial writes"));
            }
            let streams = plan_streams(b, &p, ctx, row.contract().id(), &provider_keys(providers))?;
            super::palw_monetary_material_fold_v1::enroll(b, ctx, monetary)?;
            for stream in streams {
                write_stream(b, &stream)?;
            }
            write(b, terms.root, Some(&row))?;
        }
        PalwMaterialRecoveryActionV2::Enroll { terms, providers } => {
            let m =
                b.state.monetary_material_row_v1(terms.root).ok_or_else(|| refused("recovery requires fresh monetary enrollment"))?;
            if *signer != terms.sponsor
                || terms.sponsor != m.terms.sponsor
                || m.consent != terms.initial_consent
                || m.sequence != 0
                || m.root_seen
                || m.native_binding.is_some()
                || b.state.claim(&terms.root).is_some()
                || b.state.kernel_route().and_then(|r| r.kernel_claim_row_v1(&terms.root)).is_some()
                || b.state.exec_v2_root_v1(&terms.root).is_some()
                || provider_keys(providers) != m.providers.iter().map(|p| p.provider).collect::<Vec<_>>()
                || b.state.material_recovery_row_v2(terms.root).is_some()
            {
                return Err(refused("recovery cannot retrofit history, change signers, or reuse a reserved root"));
            }
            let row = PalwMaterialRecoveryRowV2::Active(PalwMaterialRecoveryContractV2 {
                terms: terms.clone(),
                enrolled_daa: ctx.daa_score,
                generations: vec![PalwMaterialRecoveryGenerationV2 { consent: m.consent, providers: provider_keys(providers) }],
            });
            row.validate(&p).map_err(refused)?;
            let count = b
                .state
                .kernel_route()
                .map(|r| r.aux.keys().filter(|(t, k)| *t == PALW_MATERIAL_RELEASE_RECOVERY_TABLE_V2 && k.len() == 64).count())
                .unwrap_or(0);
            if count >= p.max_reserved_contracts as usize
                || (count as u64 + 1) * u64::from(p.reserved_cells()) > u64::from(p.max_reserved_history_cells)
            {
                return Err(refused("recovery receipt/history capacity unavailable before admission"));
            }
            let streams = plan_streams(b, &p, ctx, row.contract().id(), &provider_keys(providers))?;
            for stream in streams {
                write_stream(b, &stream)?;
            }
            write(b, terms.root, Some(&row))?;
        }
        PalwMaterialRecoveryActionV2::Replace { recovery, replacement, providers, .. }
        | PalwMaterialRecoveryActionV2::ReplaceBeforeV4 { recovery, replacement, providers, .. } => {
            let Some(PalwMaterialRecoveryRowV2::Active(mut c)) = b.state.material_recovery_row_v2(replacement.root) else {
                return Err(refused("replacement has no active signed recovery"));
            };
            if c.id() != *recovery
                || *signer != c.terms.sponsor
                || c.generations.len() >= c.terms.max_generations as usize
                || provider_keys(providers) != replacement.enrollment.offers.iter().map(|o| o.provider).collect::<Vec<_>>()
            {
                return Err(refused("replacement recovery consent/signers/history capacity differ"));
            }
            let new_providers: Vec<_> =
                provider_keys(providers).into_iter().filter(|p| !c.generations.iter().any(|g| g.providers.contains(p))).collect();
            let consent = a.bounded_replacement_consent_v4().unwrap_or_else(|| replacement.id());
            c.generations.push(PalwMaterialRecoveryGenerationV2 { consent, providers: provider_keys(providers) });
            let row = PalwMaterialRecoveryRowV2::Active(c);
            row.validate(&p).map_err(refused)?;
            // Existing funding/fee machinery performs all capital preflight first;
            // no earned cash, old fees or physical material obligations are erased.
            let streams = plan_streams(b, &p, ctx, row.contract().id(), &new_providers)?;
            super::palw_monetary_material_fold_v1::replace_generation_with_recovery_v4(b, ctx, replacement, consent)?;
            for stream in streams {
                write_stream(b, &stream)?;
            }
            write(b, replacement.root, Some(&row))?;
        }
        PalwMaterialRecoveryActionV2::Acknowledge { root, receipt, stream, previous, sequence } => {
            let mut checkpoint = b.state.material_provider_ack_checkpoint_v2(signer).ok_or_else(|| refused("ACK stream absent"))?;
            if checkpoint.stream != *stream
                || checkpoint.last_receipt != *previous
                || checkpoint.sequence.checked_add(1) != Some(*sequence)
                || checkpoint.reserved_acks == 0
            {
                return Err(refused("ACK stream/sequence/previous checkpoint substituted or replayed"));
            }
            let Some(PalwMaterialRecoveryRowV2::Terminal(mut r)) = b.state.material_recovery_row_v2(*root) else {
                return Err(refused("ack requires an accepted terminal receipt"));
            };
            if r.id() != *receipt || !r.providers().contains(signer) || !r.acknowledged.insert(*signer) {
                return Err(refused("ack is foreign, substituted or replayed"));
            }
            checkpoint.sequence = *sequence;
            checkpoint.reserved_acks -= 1;
            checkpoint.last_receipt = *receipt;
            write_stream(b, &checkpoint)?;
            if r.acknowledged == r.providers() {
                write(b, *root, None)?
            } else {
                write(b, *root, Some(&PalwMaterialRecoveryRowV2::Terminal(r)))?
            }
        }
        PalwMaterialRecoveryActionV2::RetireProviderStream { provider, stream, sequence } => {
            let checkpoint =
                b.state.material_provider_ack_checkpoint_v2(provider).ok_or_else(|| refused("retirement stream absent"))?;
            if provider != signer || checkpoint.stream != *stream || checkpoint.sequence != *sequence {
                return Err(refused("stream retirement signer/generation/sequence differs"));
            }
            let pending = b.state.kernel_route().is_some_and(|route| {
                route.aux.iter().any(|((table, key), bytes)| {
                    if *table != PALW_MATERIAL_RELEASE_RECOVERY_TABLE_V2 || key.len() != 64 {
                        return false;
                    }
                    match borsh::from_slice::<PalwMaterialRecoveryRowV2>(bytes) {
                        Ok(PalwMaterialRecoveryRowV2::Active(c)) => c.generations.iter().any(|g| g.providers.contains(provider)),
                        Ok(PalwMaterialRecoveryRowV2::Terminal(r)) => {
                            r.providers().contains(provider) && !r.acknowledged.contains(provider)
                        }
                        Err(_) => true,
                    }
                })
            });
            if pending || checkpoint.reserved_acks != 0 || checkpoint.reserved_answer_events != 0 {
                return Err(refused("provider stream has pending accepted obligations"));
            }
            b.write_kernel_row(PALW_MATERIAL_RELEASE_RECOVERY_TABLE_V2, material_provider_stream_key_v2(provider), None)?;
        }
    }
    Ok(())
}
pub(super) fn released(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    root: Hash64,
    consent: Hash64,
    tail: Hash64,
) -> Result<(), PalwStateV2Error> {
    let Some(row) = b.state.material_recovery_row_v2(root) else { return Ok(()) };
    let PalwMaterialRecoveryRowV2::Active(c) = row else { return Err(refused("duplicate terminal release")) };
    if c.generations.last().map(|g| g.consent) != Some(consent) {
        return Err(refused("terminal generation differs"));
    }
    let monetary =
        b.state.monetary_material_row_v1(root).ok_or_else(|| refused("released monetary row absent before terminal receipt"))?;
    let unused_native_session = monetary.terms.native_session.is_some() && monetary.native_binding.is_none() && !monetary.root_seen;
    let r = PalwMaterialTerminalReceiptV2 {
        contract: c,
        final_consent: consent,
        tail_block: tail,
        release_block: ctx.block,
        unused_native_session,
        acknowledged: BTreeSet::new(),
    };
    write(b, root, Some(&PalwMaterialRecoveryRowV2::Terminal(r)))?;
    Ok(())
}

pub(super) fn reserve_answer_event_v4(
    b: &mut TransitionBuilder<'_>,
    root: Hash64,
    provider: &PalwBondKeyV2,
) -> Result<(), PalwStateV2Error> {
    if b.state.material_recovery_row_v2(root).is_none() {
        return Ok(());
    }
    let mut c =
        b.state.material_provider_ack_checkpoint_v2(provider).ok_or_else(|| refused("challenge provider checkpoint absent"))?;
    c.reserved_answer_events = c
        .reserved_answer_events
        .checked_add(1)
        .filter(|n| c.sequence.checked_add(c.reserved_acks).and_then(|seq| seq.checked_add(*n)).is_some())
        .ok_or_else(|| refused("challenge answer sequence capacity unavailable before admission"))?;
    write_stream(b, &c)?;
    Ok(())
}
pub(super) fn release_answer_event_v4(
    b: &mut TransitionBuilder<'_>,
    root: Hash64,
    provider: &PalwBondKeyV2,
) -> Result<(), PalwStateV2Error> {
    if b.state.material_recovery_row_v2(root).is_none() {
        return Ok(());
    }
    let mut c = b
        .state
        .material_provider_ack_checkpoint_v2(provider)
        .ok_or_else(|| refused("expired challenge provider checkpoint absent"))?;
    c.reserved_answer_events = c.reserved_answer_events.checked_sub(1).ok_or_else(|| refused("expired answer credit missing"))?;
    write_stream(b, &c)?;
    Ok(())
}
