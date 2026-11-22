//! Run the projected reserve and pool moves through the native state delta and
//! carriage codecs. The fixture retains these roots outside the selected chain.
use super::*;
use kaspa_consensus_core::palw_model_market_v1::moe2::settlement::{
    Moe2ExecSlot, Moe2SettledReward, Moe2SettlementBook, Moe2SettlementUndo, Moe2WorkerRelease,
};
use kaspa_consensus_core::palw_state_v2::{
    Moe2ExecAwardV1, Moe2ExecCloseV1, Moe2ExecWorkerPayees, PalwBlockContextV2, PalwDeltaEntryV2, PalwStateCarriageV2,
    PalwStateDeltaV2, PalwStateV2Error, PalwTransitionExtrasV1, apply_delta_v2, apply_palw_transition_v2_with_extras, revert_delta_v2,
};
use kaspa_consensus_core::{
    merkle::calc_hash_merkle_root, palw_bond_budget_v1::PalwBondBudgetStateV1, palw_state_v2::apply_palw_transition_v2,
};
use std::path::Path;

pub(super) fn check_exec_settlement_rewind(
    env: &Env,
    registration: &PalwChainStateV2,
    registered_book: &Moe2SettlementBook,
    close: Hash64,
    position: Moe2Position,
    quote: Moe2PreDrawQuote,
) {
    let params = &env.bundle.state;
    let config = &env.config.params;
    let vp = env.chain.vp();
    let store = vp.palw_state_v2_store.read();
    let (_, close_delta) = store.delta_of(close).unwrap();
    drop(store);
    let slot = Moe2ExecSlot { registration: [24; 64], slot: 1, close, close_daa: close_delta.point.daa_score, quote };
    let claim = Hash64::from_u64_word(642);
    // This fixture injects the identity that the block owner's authenticated
    // EXEC verifier would provide. It only exercises native accounting.
    let work = Hash64::from_u64_word(650);
    let other_work = Hash64::from_u64_word(651);
    let alias_claim = Hash64::from_u64_word(652);
    let alias_slot = Moe2ExecSlot {
        registration: [25; 64],
        slot: 2,
        quote: Moe2PreDrawQuote { funding_bond: env.chain.bonds[0], ..quote },
        ..slot
    };
    let mut book = registered_book.clone();
    let unreserved = apply_delta_v2(registration, &close_delta, params).unwrap();
    let budget = unreserved.bond_budget().unwrap();
    let policy = &params.bond_budget().unwrap().policy;
    let release_window_at = slot.close_daa + policy.window_daa;
    for expires_at in [release_window_at, release_window_at + 1] {
        let mut sample = budget.clone();
        let (capital, ask) = slot.quote.budget_ask_at(config, &unreserved, slot.close_daa).unwrap();
        let key = slot.reservation_key();
        let mut changes = Vec::new();
        sample
            .reserve_claim_v1(
                policy,
                capital,
                key,
                slot.quote.funding_bond,
                Some(slot.quote.model),
                slot.close_daa,
                ask,
                PalwBudgetOriginV1::Moe2Exec { expires_at },
                &mut changes,
            )
            .unwrap();
        sample.release_due(release_window_at, &mut changes);
        if release_window_at < expires_at {
            assert!(sample.claim_row(&key).is_some_and(|row| row.open && row.live && !row.in_window));
            sample.check_consistency().unwrap();
            sample.release_due(expires_at, &mut changes);
        }
        assert!(sample.claim_row(&key).is_none());
        sample.check_consistency().unwrap();
    }
    let parent_root = registration.state_root();
    let input = Moe2ExecCloseV1 {
        selected_parent: registration.last_point().unwrap().block,
        parent_state_root: parent_root,
        block: close,
        blue_score: close_delta.point.blue_score,
        daa_score: close_delta.point.daa_score,
        private_from: config.private_moe2_registration_from().unwrap(),
        slots: vec![slot, alias_slot],
    };
    let fold_close = |input, enabled| {
        let extras =
            PalwTransitionExtrasV1 { private_moe2_registration: enabled, private_moe2_exec_close: input, ..Default::default() };
        apply_palw_transition_v2_with_extras(registration, params, &close_delta.point, &[], None, false, false, false, false, &extras)
    };
    let (_, blank) = fold_close(None, true).unwrap();
    let (reserved, reserved_delta) = fold_close(Some(input.clone()), true).unwrap();
    assert!(reserved_delta.entries.starts_with(&blank.entries));
    let reservation_writes = reserved_delta.entries[blank.entries.len()..].to_vec();
    for (bad_input, enabled) in [
        (input.clone(), false),
        (Moe2ExecCloseV1 { parent_state_root: Hash64::default(), ..input.clone() }, true),
        (Moe2ExecCloseV1 { slots: vec![alias_slot, slot], ..input.clone() }, true),
    ] {
        assert!(matches!(fold_close(Some(bad_input), enabled), Err(PalwStateV2Error::DeltaMismatch(_))));
    }
    let mut failing = input;
    failing.slots[1].quote.limits.max_reward = u64::MAX;
    assert!(matches!(fold_close(Some(failing), true), Err(PalwStateV2Error::BondBudgetRefused(_))));
    assert_eq!(registration.state_root(), parent_root);
    assert_eq!(revert_delta_v2(&reserved, &reserved_delta, params).unwrap().state_root(), parent_root);
    assert_eq!(apply_delta_v2(registration, &reserved_delta, params).unwrap().state_root(), reserved.state_root());
    let sibling = Moe2ExecSlot { close: Hash64::from_u64_word(647), ..slot };
    let revised = Moe2ExecSlot {
        quote: Moe2PreDrawQuote { price_limits: Moe2PriceLimits { max_output_slippage_bps: 1, ..quote.price_limits }, ..quote },
        ..slot
    };
    for different in [sibling, revised] {
        assert_eq!(
            Moe2ExecSlot::consent_ref(slot.registration, slot.slot),
            Moe2ExecSlot::consent_ref(different.registration, different.slot)
        );
        assert_ne!(slot.reservation_key(), different.reservation_key());
        assert_eq!(
            book.admit_exec_slot(claim, &different, reserved.bond_budget().unwrap(), |_, _| {
                Some(*registered_book.model(&quote.model).unwrap())
            }),
            Err(Moe2SettlementError::ReservationMismatch)
        );
        assert_eq!(&book, registered_book);
    }
    book.admit_exec_slot(claim, &slot, reserved.bond_budget().unwrap(), |anchor, id| {
        (anchor == quote.quote_anchor && id == quote.model).then(|| *registered_book.model(&id).unwrap())
    })
    .unwrap();
    let reward = quote.limits.max_reward;
    let release = Moe2WorkerRelease {
        reward,
        miner_now: reward * 4 / 10,
        panel_now: reward / 10,
        liability_until: quote.deadline,
        signed_hard_deadline: quote.deadline,
        material_ttl_until: quote.deadline,
        vest_until: None,
        retire_not_before: quote.deadline,
    };
    let panel = [(env.chain.bonds[1], release.panel_now)];
    let payees = Moe2ExecWorkerPayees { miner: env.chain.bonds[0], panel: &panel };
    assert_eq!(
        reserved.private_moe2_exec_worker_outputs(
            &kaspa_consensus_core::config::params::palw_t12_shipped_params(),
            position.point.daa_score,
            release,
            payees,
        ),
        Err(Moe2SettlementError::UnknownClaimContext),
    );
    let outputs = reserved.private_moe2_exec_worker_outputs(config, position.point.daa_score, release, payees).unwrap();
    for (output, (bond, amount)) in outputs.iter().zip([(payees.miner, release.miner_now), panel[0]]) {
        let payload = reserved.bond(&bond).unwrap().payout_payload;
        assert_eq!(output.value, amount);
        assert_eq!(output.script_public_key, kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk(&payload.as_bytes()));
    }
    assert_eq!(outputs.len(), 2);
    let execution = env.chain.ctx.consensus.get_block(position.point.block).unwrap();
    let worker_start = execution.transactions[0].outputs.len();
    let candidate = |workers: &[kaspa_consensus_core::tx::TransactionOutput]| {
        let mut transactions: Vec<_> = execution.transactions.iter().cloned().collect();
        assert!(transactions[0].is_coinbase());
        transactions[0].outputs.extend_from_slice(workers);
        transactions[0].finalize();
        let mut header = execution.header.as_ref().clone();
        header.palw_state_root = reserved.state_root();
        header.hash_merkle_root = calc_hash_merkle_root(transactions.iter());
        header.finalize();
        Block::new(header, transactions).with_evm_payload(execution.evm_payload.clone())
    };
    let fold = |block: &Block| {
        let point = PalwBlockContextV2 { block: block.hash(), ..position.point };
        let (state, delta) = apply_palw_transition_v2(&reserved, params, &point, &[], None).unwrap();
        (state, delta, Moe2Position { point, ..position })
    };
    let award_block = candidate(&outputs);
    let (mut child, mut award_delta, position) = fold(&award_block);
    let original_delta = award_delta.clone();
    let before = child.state_root();
    {
        let mut old_state = child.clone();
        let mut old_book = book.clone();
        let mut old_delta = award_delta.clone();
        old_state
            .settle_private_moe2_exec_slot(
                config,
                &mut old_book,
                &mut old_delta,
                claim,
                position,
                release,
                &slot,
                payees,
                &award_block,
                worker_start,
            )
            .unwrap();
        assert!(matches!(old_delta.entries.last(), Some(PalwDeltaEntryV2::PrivateMoe2ExecSettlement { .. })));
        let old_root = old_state.state_root();
        let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(&old_state)).unwrap();
        let old_carriage: PalwStateCarriageV2 = borsh::from_slice(&bytes).unwrap();
        assert!(old_carriage.private_moe2_work_uses.is_empty());
        assert_eq!(old_carriage.into_state(params, Some(old_root)).unwrap().state_root(), old_root);
        assert_eq!(revert_delta_v2(&old_state, &old_delta, params).unwrap().state_root(), reserved.state_root());
    }

    let mut book = registered_book.clone();
    let before_award = book.clone();
    let check_forged_journal = |parent: &PalwChainStateV2, mut state: PalwChainStateV2, mut delta: PalwStateDeltaV2, block: &Block| {
        let at = Moe2Position { point: delta.point, ..position };
        let mut book = registered_book.clone();
        book.admit_exec_slot(claim, &slot, state.bond_budget().unwrap(), |_, id| registered_book.model(&id).copied()).unwrap();
        let before = (state.state_root(), book.clone(), delta.clone());
        assert_eq!(
            state.settle_private_moe2_exec_work(
                config,
                &mut book,
                &mut delta,
                claim,
                at,
                release,
                &slot,
                work,
                payees,
                block,
                worker_start,
            ),
            Err(Moe2SettlementError::ReservationMismatch),
        );
        assert_eq!((state.state_root(), book.clone(), delta.clone()), before);

        // Produce an otherwise valid receipt/debit pair and replace only the
        // journal variant, as if restoring a forged keyed settlement.
        state
            .settle_private_moe2_exec_slot(config, &mut book, &mut delta, claim, at, release, &slot, payees, block, worker_start)
            .unwrap();
        assert_eq!(apply_delta_v2(parent, &delta, params).unwrap().state_root(), state.state_root());
        let Some(PalwDeltaEntryV2::PrivateMoe2ExecSettlement { record }) = delta.entries.pop() else { unreachable!() };
        delta.entries.push(PalwDeltaEntryV2::PrivateMoe2ExecWorkSettlement { work, record });
        let mut carried = PalwStateCarriageV2::from_state(&state);
        carried.private_moe2_work_uses.insert(work, claim);
        let restored = carried.into_state(params, None).unwrap();
        for result in [apply_delta_v2(parent, &delta, params), revert_delta_v2(&restored, &delta, params)] {
            assert!(matches!(
                result,
                Err(PalwStateV2Error::DeltaMismatch("private MOE2 EXEC reservation differs from selected parent"))
            ));
        }
    };

    // Simulate an award with a budget row that was absent from its parent.
    let mut header = award_block.header.as_ref().clone();
    header.palw_state_root = unreserved.state_root();
    header.finalize();
    let funded_in_award = Block::from_arcs(std::sync::Arc::new(header), award_block.transactions.clone())
        .with_evm_payload(award_block.evm_payload.clone());
    let point = PalwBlockContextV2 { block: funded_in_award.hash(), ..position.point };
    let (_, mut detached_delta) = apply_palw_transition_v2(&unreserved, params, &point, &[], None).unwrap();
    detached_delta.entries.extend(reservation_writes);
    let mut detached_state = apply_delta_v2(&unreserved, &detached_delta, params).unwrap();
    assert!(detached_state.bond_budget().unwrap().claim_row(&slot.reservation_key()).is_some());
    let detached_root = detached_state.state_root();
    let detached_before = detached_delta.clone();
    assert_eq!(
        detached_state.settle_private_moe2_exec_award(
            config,
            &mut book,
            &mut detached_delta,
            &unreserved,
            registration,
            claim,
            Moe2Position { point, ..position },
            release,
            &slot,
            work,
            payees,
            &funded_in_award,
            worker_start,
        ),
        Err(Moe2SettlementError::ReservationMismatch),
    );
    assert_eq!((detached_state.state_root(), &detached_delta, &book), (detached_root, &detached_before, &before_award));
    check_forged_journal(&unreserved, detached_state, detached_delta, &funded_in_award);

    use kaspa_consensus_core::palw_bond_budget_v1::{PALW_BUDGET_TABLE_CLAIMS_V1, PALW_BUDGET_TABLE_RELEASES_V1};
    let key = slot.reservation_key();
    let row = *reserved.bond_budget().unwrap().claim_row(&key).unwrap();
    let mut altered_delta = award_delta.clone();
    let earlier = row.reuse_not_before.checked_sub(1).unwrap();
    let altered = kaspa_consensus_core::palw_bond_budget_v1::PalwBudgetClaimRowV1 { reuse_not_before: earlier, ..row };
    altered_delta.entries.extend([
        PalwDeltaEntryV2::BondBudgetRow {
            table: PALW_BUDGET_TABLE_CLAIMS_V1,
            key: key.as_byte_slice().to_vec(),
            old: Some(borsh::to_vec(&row).unwrap()),
            new: Some(borsh::to_vec(&altered).unwrap()),
        },
        // Keep the window queue consistent so restore reaches settlement replay.
        PalwDeltaEntryV2::BondBudgetRow {
            table: PALW_BUDGET_TABLE_RELEASES_V1,
            key: borsh::to_vec(&(row.reuse_not_before, key)).unwrap(),
            old: Some(Vec::new()),
            new: None,
        },
        PalwDeltaEntryV2::BondBudgetRow {
            table: PALW_BUDGET_TABLE_RELEASES_V1,
            key: borsh::to_vec(&(earlier, key)).unwrap(),
            old: None,
            new: Some(Vec::new()),
        },
    ]);
    let mut altered_state = apply_delta_v2(&reserved, &altered_delta, params).unwrap();
    let altered_before = (altered_state.state_root(), altered_delta.clone());
    assert_eq!(
        altered_state.settle_private_moe2_exec_award(
            config,
            &mut book,
            &mut altered_delta,
            &reserved,
            registration,
            claim,
            position,
            release,
            &slot,
            work,
            payees,
            &award_block,
            worker_start,
        ),
        Err(Moe2SettlementError::ReservationMismatch),
    );
    assert_eq!((altered_state.state_root(), &altered_delta), (altered_before.0, &altered_before.1));
    assert_eq!(book, before_award);
    check_forged_journal(&reserved, altered_state, altered_delta, &award_block);

    let duplicate = [panel[0], panel[0]];
    assert_eq!(
        child.private_moe2_exec_worker_outputs(
            config,
            position.point.daa_score,
            release,
            Moe2ExecWorkerPayees { panel: &duplicate, ..payees }
        ),
        Err(Moe2SettlementError::WorkerOutputsMismatch),
    );
    let altered = [outputs[1].clone(), outputs[0].clone()];
    for wrong in [&[][..], &altered] {
        let block = candidate(wrong);
        let (mut state, mut delta, at) = fold(&block);
        let original = (state.state_root(), delta.clone());
        assert_eq!(
            state.settle_private_moe2_exec_award(
                config,
                &mut book,
                &mut delta,
                &reserved,
                registration,
                claim,
                at,
                release,
                &slot,
                work,
                payees,
                &block,
                worker_start
            ),
            Err(Moe2SettlementError::WorkerOutputsMismatch),
        );
        assert_eq!((state.state_root(), delta), original);
        assert_eq!(&book, &before_award);
    }
    let detached = Block::from_arcs(award_block.header.clone(), execution.transactions.clone());
    for (block, parent, price_state, start, expected) in [
        (&detached, &reserved, registration, worker_start, Moe2SettlementError::WorkerOutputsMismatch),
        (&award_block, &reserved, registration, usize::MAX, Moe2SettlementError::WorkerOutputsMismatch),
        (&execution, &reserved, registration, worker_start, Moe2SettlementError::UnknownClaimContext),
        (&award_block, &unreserved, registration, worker_start, Moe2SettlementError::UnknownClaimContext),
        (&award_block, &reserved, &unreserved, worker_start, Moe2SettlementError::UnknownClaimContext),
    ] {
        assert_eq!(
            child.settle_private_moe2_exec_award(
                config,
                &mut book,
                &mut award_delta,
                parent,
                price_state,
                claim,
                position,
                release,
                &slot,
                work,
                payees,
                block,
                start,
            ),
            Err(expected),
        );
        assert_eq!((child.state_root(), &book, &award_delta), (before, &before_award, &original_delta));
    }
    // Unrelated transactions cannot be substituted while keeping the award
    // header and the expected worker outputs.
    let mut swapped: Vec<_> = award_block.transactions.iter().cloned().collect();
    swapped.push(execution.transactions[0].clone());
    let mismatched_body = Block::from_arcs(award_block.header.clone(), swapped);
    assert_eq!(
        child.settle_private_moe2_exec_award(
            config,
            &mut book,
            &mut award_delta,
            &reserved,
            registration,
            claim,
            position,
            release,
            &slot,
            work,
            payees,
            &mismatched_body,
            worker_start,
        ),
        Err(Moe2SettlementError::WorkerOutputsMismatch),
    );
    assert_eq!((child.state_root(), &book, &award_delta), (before, &before_award, &original_delta));
    assert!(matches!(
        child.settle_private_moe2_exec_award(
            config,
            &mut book,
            &mut award_delta,
            &reserved,
            registration,
            claim,
            position,
            Moe2WorkerRelease { reward: release.reward + 1, ..release },
            &slot,
            work,
            payees,
            &award_block,
            worker_start,
        ),
        Err(Moe2SettlementError::Quote(_))
    ));
    assert_eq!((child.state_root(), &book, &award_delta), (before, &before_award, &original_delta));
    assert_eq!(
        child.settle_private_moe2_exec_award(
            config,
            &mut book,
            &mut award_delta,
            &reserved,
            registration,
            claim,
            position,
            Moe2WorkerRelease { signed_hard_deadline: quote.deadline + 1, ..release },
            &slot,
            work,
            payees,
            &award_block,
            worker_start,
        ),
        Err(Moe2SettlementError::ReservationMismatch)
    );
    assert_eq!((child.state_root(), &book, &award_delta), (before, &before_award, &original_delta));
    let (payment, undo) = child
        .settle_private_moe2_exec_award(
            config,
            &mut book,
            &mut award_delta,
            &reserved,
            registration,
            claim,
            position,
            release,
            &slot,
            work,
            payees,
            &award_block,
            worker_start,
        )
        .unwrap();
    // The candidate fold must reproduce the receipt from the native parent
    // alone, including when the node restored it from a snapshot.
    let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(&reserved)).unwrap();
    let carriage: PalwStateCarriageV2 = borsh::from_slice(&bytes).unwrap();
    let restored = carriage.into_state(params, Some(reserved.state_root())).unwrap();
    let (mut native, mut native_delta) = apply_palw_transition_v2(&restored, params, &position.point, &[], None).unwrap();
    let native_award = |claim, slot, work, position| Moe2ExecAwardV1 { claim, position, release, slot, work, payees, worker_start };
    let folded = native
        .fold_private_moe2_exec_award(
            config,
            &mut native_delta,
            &restored,
            registration,
            &award_block,
            native_award(claim, &slot, work, position),
        )
        .unwrap();
    assert_eq!((folded, native.state_root(), &native_delta), (payment, child.state_root(), &award_delta));
    let before_duplicate = (native.state_root(), native_delta.clone());
    assert_eq!(
        native.fold_private_moe2_exec_award(
            config,
            &mut native_delta,
            &restored,
            registration,
            &award_block,
            native_award(alias_claim, &alias_slot, work, position),
        ),
        Err(Moe2SettlementError::WorkAlreadyConsumed)
    );
    assert_eq!(
        native.fold_private_moe2_exec_award(
            config,
            &mut native_delta,
            &restored,
            registration,
            &award_block,
            native_award(alias_claim, &alias_slot, other_work, position),
        ),
        Err(Moe2SettlementError::PositionOutOfOrder)
    );
    assert_eq!(
        native.fold_private_moe2_exec_award(
            config,
            &mut native_delta,
            &restored,
            registration,
            &award_block,
            native_award(alias_claim, &slot, other_work, position),
        ),
        Err(Moe2SettlementError::ReservationMismatch)
    );
    assert_eq!((native.state_root(), &native_delta), (before_duplicate.0, &before_duplicate.1));
    assert_eq!(revert_delta_v2(&native, &native_delta, params).unwrap().state_root(), restored.state_root());
    let undo: Moe2SettlementUndo = borsh::from_slice(&borsh::to_vec(&undo).unwrap()).unwrap();
    let delta_bytes = borsh::to_vec(&award_delta).unwrap();
    kaspa_consensus_core::palw_native_resource_v1::preflight_native_value_v1::<PalwStateDeltaV2>(
        delta_bytes.as_slice(),
        delta_bytes.len() as u64,
    )
    .unwrap();
    award_delta = borsh::from_slice(&delta_bytes).unwrap();
    let after = child.state_root();
    assert_eq!(apply_delta_v2(&reserved, &award_delta, params).unwrap().state_root(), after);

    let mut false_purchase = award_delta.clone();
    let Some(PalwDeltaEntryV2::PrivateMoe2ExecWorkSettlement { record, .. }) = false_purchase.entries.last_mut() else {
        panic!("missing EXEC settlement after the market write");
    };
    record.reward.allocation.market.as_mut().unwrap().purchased_units += 1;
    record.permanent_liquidity.as_mut().unwrap().purchased_units += 1;
    assert!(matches!(
        apply_delta_v2(&reserved, &false_purchase, params),
        Err(PalwStateV2Error::DeltaMismatch("private MOE2 EXEC settlement lacks its same-block debit"))
    ));
    assert!(revert_delta_v2(&child, &false_purchase, params).is_err());

    let mut substituted_slot = award_delta.clone();
    let Some(PalwDeltaEntryV2::PrivateMoe2ExecWorkSettlement { record, .. }) = substituted_slot.entries.last_mut() else {
        panic!("missing EXEC settlement after the market write");
    };
    record.signed_slot = Hash64::from_u64_word(648);
    for result in [apply_delta_v2(&reserved, &substituted_slot, params), revert_delta_v2(&child, &substituted_slot, params)] {
        assert!(matches!(
            result,
            Err(PalwStateV2Error::DeltaMismatch("private MOE2 EXEC slot does not bind the consumed reservation"))
        ));
    }

    let mut carried = PalwStateCarriageV2::from_state(&child);
    let mut false_release = carried.clone();
    false_release.private_moe2_exec_settlements.get_mut(&claim).unwrap().reward.miner_now += 1;
    assert!(matches!(false_release.into_state(params, None), Err(PalwStateV2Error::CarriageInconsistent(_))));
    let mut duplicate_slot = carried.clone();
    let mut duplicate = duplicate_slot.private_moe2_exec_settlements[&claim].clone();
    duplicate.claim = Hash64::from_u64_word(649);
    duplicate.permanent_liquidity.as_mut().unwrap().claim = duplicate.claim;
    duplicate_slot.private_moe2_exec_settlements.insert(duplicate.claim, duplicate);
    assert!(matches!(
        duplicate_slot.into_state(params, None),
        Err(PalwStateV2Error::CarriageInconsistent(ref why)) if why == "inconsistent private MOE2 EXEC settlement"
    ));
    let carried_bytes = borsh::to_vec(&carried).unwrap();
    kaspa_consensus_core::palw_native_resource_v1::preflight_native_carriage_v1(carried_bytes.as_slice(), carried_bytes.len() as u64)
        .unwrap();
    let reopened: PalwStateCarriageV2 = borsh::from_slice(&carried_bytes).unwrap();
    child = reopened.into_state(params, Some(after)).unwrap();
    let receipt = child.private_moe2_exec_settlement(&claim).unwrap().clone();
    assert_eq!((receipt.reservation, receipt.reward, receipt.position), (slot.reservation_key(), payment, position));
    assert_eq!(receipt.permanent_liquidity.as_ref(), book.receipt(&claim));
    assert_eq!(child.private_moe2_work_claim(&work), Some(&claim));
    let mut conflicting_work = carried.clone();
    conflicting_work.private_moe2_work_uses.insert(other_work, claim);
    assert!(matches!(conflicting_work.into_state(params, None), Err(PalwStateV2Error::CarriageInconsistent(_))));

    // Another signed slot has independent funds and a separate claim ID, but
    // its canonical work identity is already consumed on this branch.
    let mut conflict_delta = award_delta.clone();
    let mut conflict_book = book.clone();
    assert_eq!(
        child.settle_private_moe2_exec_award(
            config,
            &mut conflict_book,
            &mut conflict_delta,
            &reserved,
            registration,
            alias_claim,
            position,
            release,
            &alias_slot,
            work,
            payees,
            &award_block,
            worker_start,
        ),
        Err(Moe2SettlementError::WorkAlreadyConsumed),
    );
    assert_eq!((child.state_root(), conflict_book, conflict_delta), (after, book.clone(), award_delta.clone()));
    // A receipt at a different block must not use the already consumed
    // reservation from the parent, even after restoring the native snapshot.
    let later = PalwBlockContextV2 { block: Hash64::from_u64_word(645), blue_score: position.point.blue_score + 1, ..position.point };
    let mut duplicate = receipt;
    duplicate.claim = Hash64::from_u64_word(646);
    duplicate.position.point = later;
    if let Some(lp) = duplicate.permanent_liquidity.as_mut() {
        lp.claim = duplicate.claim;
        lp.position = duplicate.position;
    }
    let unpaired = PalwStateDeltaV2 {
        point: later,
        entries: vec![
            PalwDeltaEntryV2::LastPoint { old: Some(position.point), new: Some(later) },
            PalwDeltaEntryV2::PrivateMoe2ExecSettlement { record: Box::new(duplicate) },
        ],
    };
    assert!(matches!(
        apply_delta_v2(&child, &unpaired, params),
        Err(PalwStateV2Error::DeltaMismatch("private MOE2 EXEC settlement lacks its same-block debit"))
    ));
    assert_eq!(child.state_root(), after);
    let mut omitted_work = carried.clone();
    omitted_work.private_moe2_work_uses.clear();
    assert!(omitted_work.into_state(params, Some(after)).is_err());
    carried.private_moe2_exec_settlements.clear();
    assert!(carried.into_state(params, None).is_err(), "work without its settlement cannot be restored");
    let disconnected = revert_delta_v2(&child, &award_delta, params).unwrap();
    assert_eq!(disconnected.state_root(), reserved.state_root());
    assert!(disconnected.private_moe2_exec_settlement(&claim).is_none());
    assert!(disconnected.private_moe2_work_claim(&work).is_none());
    let mut unkeyed = award_delta.clone();
    let Some(PalwDeltaEntryV2::PrivateMoe2ExecWorkSettlement { record, .. }) = unkeyed.entries.pop() else {
        panic!("missing numerical work settlement");
    };
    unkeyed.entries.push(PalwDeltaEntryV2::PrivateMoe2ExecSettlement { record });
    assert!(revert_delta_v2(&child, &unkeyed, params).is_err());

    let (book_root, bytes) = book.checkpoint(after);
    book = Moe2SettlementBook::from_checkpoint(after, book_root, &bytes, bytes.len()).unwrap();
    let paid_book = book.clone();
    let mut sibling = award_delta.clone();
    sibling.point.block = Hash64::from_u64_word(643);
    assert_eq!(
        child.revert_private_moe2_exec_award(config, &mut book, &mut sibling, &slot, &undo),
        Err(Moe2SettlementError::UndoMismatch)
    );
    let mut truncated = award_delta.clone();
    truncated.entries.pop();
    assert_eq!(
        child.revert_private_moe2_exec_award(config, &mut book, &mut truncated, &slot, &undo),
        Err(Moe2SettlementError::UndoMismatch)
    );
    let other = Moe2ExecSlot { close: Hash64::from_u64_word(644), ..slot };
    assert_eq!(
        child.revert_private_moe2_exec_award(config, &mut book, &mut award_delta, &other, &undo),
        Err(Moe2SettlementError::UndoMismatch)
    );
    assert_eq!((child.state_root(), &book), (after, &paid_book));
    child.revert_private_moe2_exec_award(config, &mut book, &mut award_delta, &slot, &undo).unwrap();
    assert!(child.private_moe2_work_claim(&work).is_none());
    assert_eq!((child.state_root(), &book, &award_delta.entries), (before, &before_award, &original_delta.entries));
    assert_eq!(revert_delta_v2(&child, &award_delta, params).unwrap().state_root(), reserved.state_root());
    assert!(child.revert_private_moe2_exec_award(config, &mut book, &mut award_delta, &slot, &undo).is_err());
    // Once the first settlement leaves the branch, the other independently
    // funded slot may consume the same work identity.
    let mut header = award_block.header.as_ref().clone();
    header.nonce ^= 1;
    header.finalize();
    let sibling = Block::new(header, award_block.transactions.as_ref().to_vec()).with_evm_payload(award_block.evm_payload.clone());
    let (mut replacement, mut replacement_delta, replacement_at) = fold(&sibling);
    replacement
        .fold_private_moe2_exec_award(
            config,
            &mut replacement_delta,
            &reserved,
            registration,
            &sibling,
            native_award(alias_claim, &alias_slot, work, replacement_at),
        )
        .unwrap();
    assert_eq!(replacement.private_moe2_work_claim(&work), Some(&alias_claim));
    assert_eq!(apply_delta_v2(&reserved, &replacement_delta, params).unwrap().state_root(), replacement.state_root());

    let (repaid, _) = child
        .settle_private_moe2_exec_award(
            config,
            &mut book,
            &mut award_delta,
            &reserved,
            registration,
            claim,
            position,
            release,
            &slot,
            work,
            payees,
            &award_block,
            worker_start,
        )
        .unwrap();
    assert_eq!((repaid, child.state_root(), book), (payment, after, paid_book));
    let parent = child.last_point().unwrap();
    let at_deadline = PalwBlockContextV2 {
        block: Hash64::from_u64_word(655),
        daa_score: quote.deadline,
        blue_score: parent.blue_score + 1,
        ..*parent
    };
    let (at_deadline_state, _) = apply_palw_transition_v2(&child, params, &at_deadline, &[], None).unwrap();
    let live_budget = at_deadline_state.bond_budget().unwrap();
    for slot in [&slot, &alias_slot] {
        assert!(live_budget.claim_row(&slot.reservation_key()).is_some_and(|row| row.open && row.live));
    }
    let next = PalwBlockContextV2 {
        block: Hash64::from_u64_word(656),
        daa_score: quote.deadline + 1,
        blue_score: at_deadline.blue_score + 1,
        ..at_deadline
    };
    let (expired, expire_delta) = apply_palw_transition_v2(&at_deadline_state, params, &next, &[], None).unwrap();
    for slot in [&slot, &alias_slot] {
        let key = slot.reservation_key();
        let before = live_budget.claim_row(&key).unwrap();
        let after = expired.bond_budget().unwrap().claim_row(&key);
        if before.reuse_not_before <= next.daa_score {
            assert!(after.is_none());
        } else {
            assert!(after.is_some_and(|row| !row.open && !row.live && row.in_window && row.consumed == before.consumed));
        }
    }
    assert_eq!(expired.private_moe2_work_claim(&work), Some(&claim));
    assert_eq!(expired.private_moe2_exec_settlement(&claim), child.private_moe2_exec_settlement(&claim));
    let carriage = borsh::to_vec(&PalwStateCarriageV2::from_state(&expired)).unwrap();
    let restored =
        borsh::from_slice::<PalwStateCarriageV2>(&carriage).unwrap().into_state(params, Some(expired.state_root())).unwrap();
    assert_eq!(revert_delta_v2(&restored, &expire_delta, params).unwrap().state_root(), at_deadline_state.state_root());
    assert_eq!(apply_delta_v2(&at_deadline_state, &expire_delta, params).unwrap().state_root(), expired.state_root());
    let market = payment.allocation.market.unwrap();
    println!(
        "MOE2 EXEC rollback pid={} block={} before={before} settled={after} book={book_root} purchase={} lp={} burned={} miner={} panel={}",
        std::process::id(),
        position.point.block,
        market.split.purchase,
        market.split.liquidity,
        market.retired_units,
        payment.miner_now,
        payment.panel_now,
    );
}

pub(super) struct NativeSettlement<'a> {
    store: &'a Path,
    pub(super) parent_root: Hash64,
    pub(super) child_root: Hash64,
}

impl<'a> NativeSettlement<'a> {
    pub(super) fn project(
        checkpoint_dir: &'a Path,
        env: &Env,
        registration: &PalwChainStateV2,
        enrollment: Hash64,
        reservation: &[PalwBudgetWriteV1],
        book: &mut Moe2SettlementBook,
        terms: &Moe2SettlementTerms,
        at: Moe2Position,
        release: Moe2WorkerRelease,
    ) -> (Self, Moe2SettledReward, Moe2SettlementUndo) {
        let vp = env.chain.vp();
        let store = vp.palw_state_v2_store.read();
        let (published_parent, mut enrollment) = store.delta_of(enrollment).unwrap();
        let (published_child, mut executed) = store.delta_of(at.point.block).unwrap();
        drop(store);
        let params = &env.bundle.state;
        let canonical_parent = apply_delta_v2(registration, &enrollment, params).unwrap();
        assert_eq!(canonical_parent.state_root(), published_parent);
        let mut unfunded = apply_delta_v2(&canonical_parent, &executed, params).unwrap();
        assert_eq!(unfunded.state_root(), published_child);
        let admitted = book.clone();
        let base_len = executed.entries.len();
        assert_eq!(
            unfunded.settle_private_moe2(&env.config.params, book, &mut executed, terms.claim, at, release),
            Err(Moe2SettlementError::ReservationMismatch)
        );
        assert_eq!(unfunded.state_root(), published_child);
        assert_eq!(*book, admitted);
        assert_eq!(executed.entries.len(), base_len);

        enrollment.entries.extend(reservation.iter().cloned().map(PalwDeltaEntryV2::from));
        let parent = apply_delta_v2(registration, &enrollment, params).unwrap();
        let mut child = apply_delta_v2(&parent, &executed, params).unwrap();
        let before_settlement = child.state_root();
        let wrong_release = Moe2WorkerRelease { panel_now: release.panel_now + 1, ..release };
        assert!(child.settle_private_moe2(&env.config.params, book, &mut executed, terms.claim, at, wrong_release).is_err());
        let public = kaspa_consensus_core::config::params::palw_t12_shipped_params();
        assert_eq!(
            child.settle_private_moe2(&public, book, &mut executed, terms.claim, at, release),
            Err(Moe2SettlementError::UnknownClaimContext)
        );
        assert_eq!((child.state_root(), executed.entries.len(), &*book), (before_settlement, base_len, &admitted));
        let (reward, undo) = child.settle_private_moe2(&env.config.params, book, &mut executed, terms.claim, at, release).unwrap();
        assert_eq!(book.entitlement(&terms.claim), Some(&reward));
        let child_root = child.state_root();
        assert_eq!(apply_delta_v2(&parent, &executed, params).unwrap().state_root(), child_root);
        assert_eq!(
            child.settle_private_moe2(&env.config.params, book, &mut executed, terms.claim, at, release),
            Err(Moe2SettlementError::AlreadySettled)
        );
        assert_eq!((child.state_root(), executed.entries.len()), (child_root, base_len + 2));
        assert_ne!(parent.state_root(), published_parent);
        assert_ne!(child.state_root(), published_child);
        std::fs::write(checkpoint_dir.join("native-carriage"), borsh::to_vec(&PalwStateCarriageV2::from_state(&child)).unwrap())
            .unwrap();
        std::fs::write(checkpoint_dir.join("enrollment.delta"), borsh::to_vec(&enrollment).unwrap()).unwrap();
        std::fs::write(checkpoint_dir.join("settlement.delta"), borsh::to_vec(&executed).unwrap()).unwrap();
        (Self { store: checkpoint_dir, parent_root: parent.state_root(), child_root }, reward, undo)
    }

    pub(super) fn reload(&self, env: &Env) -> PalwChainStateV2 {
        let carriage: PalwStateCarriageV2 = borsh::from_slice(&std::fs::read(self.store.join("native-carriage")).unwrap()).unwrap();
        carriage.into_state(&env.bundle.state, Some(self.child_root)).unwrap()
    }

    pub(super) fn disconnect_and_replay(
        &self,
        env: &Env,
        registration: &PalwChainStateV2,
        child: &PalwChainStateV2,
        reverted_budget: &PalwBondBudgetStateV1,
        inverse: &[PalwBudgetWriteV1],
    ) {
        let params = &env.bundle.state;
        let enrollment: PalwStateDeltaV2 = borsh::from_slice(&std::fs::read(self.store.join("enrollment.delta")).unwrap()).unwrap();
        let executed: PalwStateDeltaV2 = borsh::from_slice(&std::fs::read(self.store.join("settlement.delta")).unwrap()).unwrap();
        let parent = revert_delta_v2(child, &executed, params).unwrap();
        assert_eq!(parent.state_root(), self.parent_root);
        let PalwDeltaEntryV2::BondBudgetRow { table, key, old, new } = &executed.entries[executed.entries.len() - 2] else {
            panic!("native debit")
        };
        assert_eq!(inverse, &[PalwBudgetWriteV1::Row { table: *table, key: key.clone(), old: new.clone(), new: old.clone() }]);
        // The block may also have journaled ordinary accrual. Compare the
        // budgets at the same position before disconnecting the whole block.
        let mut before_move = executed.clone();
        before_move.entries.truncate(before_move.entries.len() - 2);
        assert_eq!(apply_delta_v2(&parent, &before_move, params).unwrap().bond_budget(), Some(reverted_budget));
        assert_eq!(revert_delta_v2(&parent, &enrollment, params).unwrap().state_root(), registration.state_root());
        assert_eq!(apply_delta_v2(&parent, &executed, params).unwrap().state_root(), self.child_root);
    }
}
