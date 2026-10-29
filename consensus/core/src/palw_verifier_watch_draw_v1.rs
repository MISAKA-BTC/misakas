//! Frozen epoch pools and post-commit sealed-v3 ordinary claim draws.
use super::*;
use crate::palw_verifier_watch_v1::*;
fn bad(why: impl ToString) -> PalwStateV2Error {
    PalwStateV2Error::KernelRouteRefused(why.to_string())
}
fn read<T: borsh::BorshDeserialize + borsh::BorshSerialize + crate::palw_native_resource_v1::NativeWireScanV1>(
    bytes: &[u8],
) -> Result<crate::palw_native_resource_v1::NativeRetainedV1<T>, PalwStateV2Error> {
    crate::palw_claim_local_v1::decode_claim_local_value_v1(bytes).map_err(|e| match e {
        crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(e) => PalwStateV2Error::NativeResource(e),
        crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(e) => bad(e),
    })
}
fn reserve(builder: &mut TransitionBuilder<'_>, rows: usize) -> Result<(), PalwStateV2Error> {
    let mut q = native_writers::NativeWriteQuoteV1::new("frozen watcher pools and contexts");
    q.exact::<PalwFrozenWatcherV1>(rows as u64)?;
    q.exact::<[u8; 65]>(rows as u64)?;
    q.exact::<PalwWatcherClaimContextV1>(rows as u64)?;
    builder.admit_native_write_v1(q)?;
    Ok(())
}
/// Pre-object hook: membership is not selected by the first producing claim.
/// Entries accepted within this epoch only start in a later epoch.
pub(super) fn freeze_epoch(builder: &mut TransitionBuilder<'_>, point: &PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
    let Some((_, policy)) = builder.params.verifier_watch_v1.as_ref().filter(|(at, _)| point.daa_score >= *at) else {
        return Ok(());
    };
    let policy = policy.clone();
    policy.validate().map_err(bad)?;
    let role =
        builder.params.rfc0018_v1().ok_or_else(|| bad("watcher pool needs the existing native verifier maturity policy"))?;
    let (native_maturity, early) =
        builder.params.verifier_watch_maturity_v1.ok_or_else(|| bad("watcher native maturity authority absent"))?;
    let native_window = crate::config::params::palw_bond_maturity_window_in_force_v1(native_maturity, early, point.daa_score);
    let mature_window = palw_bond_maturity_window_on_state_v1(
        &builder.state,
        Some(builder.params.window_court()),
        point.daa_score,
        native_window,
        builder.extras.settled_anchor_depth,
    )
    .unwrap_or(0)
    .max(role.certification.verifier_maturity_daa);
    let verifier_minimum = role.certification.verifier_min_collateral_sompi;
    let epoch = point.daa_score / policy.epoch_length_daa;
    super::palw_kernel_route_fold_v1::ensure_route_header(builder, point)?;
    let count = builder.state.kernel_route.as_ref().unwrap().aux.len();
    reserve(builder, count)?;
    let route = builder.state.kernel_route.as_ref().unwrap();
    let mut classes: BTreeMap<[u8; 64], BTreeMap<[u8; 64], PalwFrozenWatcherV1>> = BTreeMap::new();
    for ((t, k), bytes) in &route.aux {
        if *t != TABLE || k.len() != 65 || k[0] != WATCH {
            continue;
        }
        let row: PalwVerifierWatchRowV1 = borsh::from_slice(bytes).map_err(bad)?;
        if epoch < row.terms.first_epoch || epoch > row.terms.last_epoch {
            continue;
        }
        let id = pool_id(row.terms.class, epoch, policy.id());
        if route.aux.contains_key(&(TABLE, key(POOL, &id).to_vec())) {
            continue;
        }
        let pool = classes.entry(row.terms.class).or_default();
        let actor = route.bond_key_of(&row.terms.watcher).ok_or_else(|| bad("frozen watcher actor missing"))?;
        let bond = builder.state.bonds.get(&actor).ok_or_else(|| bad("frozen watcher native bond missing"))?;
        if !matches!(bond.status, PalwBondStatusV2::Active)
            || bond.registered_daa.checked_add(mature_window).is_none_or(|at| at > point.daa_score)
            || bond.collateral < verifier_minimum
            || builder.state.verifier_watch_reserved_at_checked_v1(&actor, point.daa_score)? > u128::from(bond.collateral)
        {
            continue;
        }
        let m = pool.entry(row.terms.watcher).or_insert(PalwFrozenWatcherV1 {
            watcher: row.terms.watcher,
            operator: bond.operator_id,
            registered_daa: bond.registered_daa,
            stake: 0,
        });
        m.stake = m.stake.checked_add(row.terms.stake).ok_or_else(|| bad("frozen watcher stake overflow"))?;
    }
    for (class, members) in classes {
        let row = PalwFrozenWatcherPoolV1 {
            class,
            epoch,
            frozen_daa: point.daa_score,
            maturity_window: mature_window,
            policy: policy.id(),
            members: members.into_values().collect(),
        };
        builder.write_kernel_row_encoded_v1(TABLE, &key(POOL, &row.id()), Some(&row))?;
    }
    Ok(())
}

pub(super) fn tick(builder: &mut TransitionBuilder<'_>, point: &PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
    let Some((from, policy)) = builder.params.verifier_watch_v1.as_ref().filter(|(at, _)| point.daa_score >= *at) else {
        return Ok(());
    };
    let from = *from;
    let policy = policy.clone();
    policy.validate().map_err(bad)?;
    let Some(extras) = builder.extras.kernel_route.clone() else {
        return Ok(());
    };
    super::palw_kernel_route_fold_v1::reserve_route_rows_copy_v1(builder)?;
    let count = builder.state.kernel_route.as_ref().map_or(0, |r| r.rows.len() + r.aux.len());
    reserve(builder, count)?;
    let route = builder.state.kernel_route.as_ref().ok_or_else(|| bad("watcher draw route absent"))?;
    let ledger =
        route.ledger_checked_v1().map_err(|e| crate::palw_kernel_route_v1::kernel_ledger_projection_state_error_v1(e, bad))?;
    let mut births = Vec::new();
    for (id, c) in &ledger.claims {
        if c.committed_daa < from || c.committed_daa != point.daa_score || route.aux.contains_key(&(TABLE, key(CONTEXT, id).to_vec()))
        {
            continue;
        }
        if route
            .paid_claim_authority_for_base_checked_v1(id)
            .map_err(|e| crate::palw_kernel_route_v1::kernel_ledger_projection_state_error_v1(e, bad))?
            .is_some()
        {
            continue;
        }
        if !ledger.verifier_pay.contains_key(&(misaka_palw_kernel::verifier_pay::VERIFIER_PAY_FEE_ESCROW_V1, c.job_id)) {
            continue;
        }
        let epoch = c.committed_daa / policy.epoch_length_daa;
        let (pool, pool_lease) = match route.aux.get(&(TABLE, key(POOL, &pool_id(c.class_binding_id, epoch, policy.id())).to_vec())) {
            Some(bytes) => read::<PalwFrozenWatcherPoolV1>(bytes)?.into_parts_v1(),
            None => (
                PalwFrozenWatcherPoolV1 {
                    class: c.class_binding_id,
                    epoch,
                    frozen_daa: point.daa_score,
                    maturity_window: 0,
                    policy: policy.id(),
                    members: Vec::new(),
                },
                None,
            ),
        };
        let actor = route.bond_key_of(&c.producer).ok_or_else(|| bad("watcher claim producer missing"))?;
        let excluded_operator = builder.state.bonds.get(&actor).ok_or_else(|| bad("watcher producer native bond absent"))?.operator_id;
        let eligible = extras
            .opv
            .as_ref()
            .map(|opv| {
                route.opv_eligible_set_v1(&ledger, point.daa_score, &crate::palw_opv_bootstrap_v1::OpvEligibilityViewV1::of(opv))
            })
            .unwrap_or_default();
        let hard =
            super::palw_settlement_liability_fold_v1::kernel_claim_signed_hard_deadline_v1(builder.params, c.committed_daa, None)
                .map_err(bad)?;
        if hard.checked_sub(c.committed_daa).is_none_or(|n| n > policy.maximum_claim_horizon_daa) {
            return Err(bad("watcher stake horizon is shorter than original X3"));
        }
        let beacon = misaka_palw_challenge::BeaconContextV1 {
            chain_genesis: extras.chain_genesis.as_bytes(),
            ruleset_id: route.header.policy.ruleset_digest,
            policy: policy.beacon.clone(),
            subject_kind: misaka_palw_challenge::SubjectKindV1::ClaimVerification,
            commitment_root: *id,
            commitment_position: c.committed_daa,
            challenge_epoch: epoch,
            eligible_profiles: eligible.into_iter().filter(|x| x.as_bytes() != c.class_binding_id).map(|x| x.as_bytes()).collect(),
            excluded_profiles: BTreeSet::from([c.class_binding_id]),
            candidate_profile_id: misaka_palw_challenge::RootV1::Absent,
        };
        births.push((
            PalwWatcherClaimContextV1 {
                claim: *id,
                pool,
                excluded_operator,
                beacon,
                hard_deadline: hard,
                selected: None,
                complete: false,
            },
            pool_lease,
        ));
    }
    drop(ledger);
    for (context, _pool_lease) in births {
        builder.write_kernel_row_encoded_v1(TABLE, &key(CONTEXT, &context.claim), Some(&context))?;
    }
    let route = builder.state.kernel_route.as_ref().unwrap();
    let keys: Vec<[u8; 65]> = route
        .aux
        .keys()
        .filter(|(t, k)| *t == TABLE && k.len() == 65 && k[0] == CONTEXT)
        .map(|(_, k)| k.as_slice().try_into().unwrap())
        .collect();
    let sources = route.beacon_sealed_sources_v1().map_err(bad)?;
    for k in keys {
        let (mut context, _context_lease) =
            read::<PalwWatcherClaimContextV1>(&builder.state.kernel_route.as_ref().unwrap().aux[&(TABLE, k.to_vec())])?
                .into_parts_v1();
        if context.complete {
            if point.daa_score > context.hard_deadline {
                builder.write_kernel_row_encoded_v1::<PalwWatcherClaimContextV1>(TABLE, &k, None)?;
            }
            continue;
        }
        let expired = point.daa_score > context.hard_deadline;
        if !expired && context.selected.is_none() {
            match misaka_palw_challenge::collect_sealed_work_beacon_v3(&context.beacon, &sources, point.daa_score).map_err(bad)? {
                misaka_palw_challenge::SealedBeaconStateV3::Locked(beacon) => {
                    context.selected = Some(draw_watchers_v1(&policy, &beacon.output, &context).map_err(bad)?)
                }
                misaka_palw_challenge::SealedBeaconStateV3::Unavailable { .. }
                | misaka_palw_challenge::SealedBeaconStateV3::Vetoed { .. } => context.selected = Some(Vec::new()),
                _ => continue,
            }
            builder.write_kernel_row_encoded_v1(TABLE, &k, Some(&context))?;
        }
        let probe = super::palw_kernel_route_fold_v1::load_ledger(builder, point)?;
        if probe.verifier_pay.contains_key(&(misaka_palw_kernel::verifier_pay::VERIFIER_PAY_DRAW_V1, context.claim)) {
            context.complete = true;
            builder.write_kernel_row_encoded_v1(TABLE, &k, Some(&context))?;
            continue;
        }
        let claim = probe.claims.get(&context.claim).ok_or_else(|| bad("watcher context lost its claim"))?;
        let deadline = draw_deadline(&claim.life.state);
        let expired = expired || claim.life.state.is_terminal() || deadline.is_some_and(|at| point.daa_score > at);
        if !expired && deadline.is_none() {
            continue;
        }
        if !expired
            && !context.selected.as_ref().is_some_and(Vec::is_empty)
            && probe.verifier_pay_policy.is_none_or(|p| p.slots != policy.slots)
        {
            return Err(bad("explicit watcher slots differ from actual job fee escrow policy"));
        }
        drop(probe);
        let checkpoint = builder.try_checkpoint()?;
        let result = (|| {
            let mut ledger = super::palw_kernel_route_fold_v1::load_ledger(builder, point)?;
            let before = super::palw_kernel_route_fold_v1::loaded_rows(builder, &ledger)?;
            let slots = context.selected.as_deref().unwrap_or(&[]);
            let events = if expired || slots.is_empty() {
                ledger.cancel_undrawn_check_fee_reserved_v1(&context.claim).map_err(PalwStateV2Error::NativeResource)?.map_err(bad)?
            } else {
                ledger.assign_drawn_reserved_v1(&context.claim, slots).map_err(PalwStateV2Error::NativeResource)?.map_err(bad)?
            };
            if let Some(misaka_palw_kernel::verifier_pay::VerifierPayRowV1::Draw { fee, .. }) =
                ledger.verifier_pay.get(&(misaka_palw_kernel::verifier_pay::VERIFIER_PAY_DRAW_V1, context.claim))
            {
                let fee = *fee;
                if fee > 0 {
                    for (slot, actor) in slots.iter().enumerate() {
                        let bond = builder
                            .state
                            .kernel_route
                            .as_ref()
                            .unwrap()
                            .bond_key_of(actor)
                            .ok_or_else(|| bad("drawn watcher native actor absent"))?;
                        builder.reserve_source_income_v1(
                            point,
                            crate::palw_verifier_fee_consent_v1::verifier_fee_income_context_v1(context.claim, *actor, slot as u64),
                            Hash64::from_bytes(context.claim),
                            bond,
                            context.hard_deadline,
                            fee,
                        )?;
                    }
                }
            }
            super::palw_kernel_route_fold_v1::apply_settlements(builder, &events, true, point.daa_score)?;
            super::palw_kernel_route_fold_v1::flush(builder, &ledger, &before)?;
            Ok::<(), PalwStateV2Error>(())
        })();
        match result {
            Ok(()) => {
                context.complete = true;
                builder.write_kernel_row_encoded_v1(TABLE, &k, Some(&context))?;
            }
            Err(PalwStateV2Error::BondBudgetExhausted { .. }) => {
                builder.restore(checkpoint);
            }
            Err(e) => {
                builder.restore(checkpoint);
                return Err(e);
            }
        }
    }
    Ok(())
}

fn draw_deadline(state: &misaka_palw_kernel::lifecycle::ClaimStateV1) -> Option<u64> {
    use misaka_palw_kernel::lifecycle::ClaimStateV1 as S;
    match state {
        S::Checking { deadline_daa, .. } => Some(*deadline_daa),
        S::ProbabilisticPass { window_end_daa, .. } | S::Challengeable { window_end_daa, .. } => Some(*window_end_daa),
        S::Disputed { resume, .. } => draw_deadline(resume),
        _ => None,
    }
}
