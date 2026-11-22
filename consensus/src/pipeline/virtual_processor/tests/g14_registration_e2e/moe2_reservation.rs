//! Isolated accounting projection on each peer's selected registration state.
use super::*;
use crate::pipeline::virtual_processor::tests::new_miner_data;
use kaspa_consensus_core::{
    constants::SOMPI_PER_KASPA,
    palw_bond_budget_v1::{PalwBudgetAskV1, PalwBudgetOriginV1, PalwBudgetWriteV1, palw_bond_budget_export_cap_v1},
    palw_model_market_v1::moe2::{
        Moe2PriceLimits, Moe2QuantityLimits, Moe2WorkerPlan,
        settlement::{
            Moe2Position, Moe2PreDrawQuote, Moe2SettlementError, Moe2SettlementFunding, Moe2SettlementTerms, Moe2WorkerRelease,
        },
    },
    palw_settlement_liability_v1::PalwSettlementTermsSourcedV1,
};
use libcrux_ml_dsa::ml_dsa_87;
use misaka_palw_remote::private_moe2::PrivateMoe2Checkpoint;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) struct Observation {
    enrollment: Hash64,
    checkpoint: Hash64,
    carrier: Hash64,
    position: Hash64,
    reservation: Hash64,
    consumed: u64,
    reserve: u64,
    retired: u64,
    miner: u64,
    panel: u64,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub(super) struct Replay {
    blocks: [RpcRawBlock; 3],
    signature: Vec<u8>,
    undo: Vec<u8>,
}

pub(in crate::pipeline::virtual_processor::tests) fn registration_quote(
    state: &PalwChainStateV2,
    model: Hash64,
    bond: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
    policy: Hash64,
) -> Moe2PreDrawQuote {
    let point = state.last_point().unwrap();
    let reward = 160 * SOMPI_PER_KASPA;
    Moe2PreDrawQuote {
        funding_bond: bond,
        model,
        artifact_root: state.class(&model).unwrap().artifact_root,
        policy,
        quote_anchor: point.block,
        valid_from: point.daa_score,
        deadline: point.daa_score + 5,
        plan: Moe2WorkerPlan::MinerAndPanel20,
        limits: Moe2QuantityLimits {
            max_reward: reward,
            max_purchase_sompi: reward * 3 / 10,
            min_out: 2,
            max_purchase_units: 2,
            max_lp_sompi: reward / 5,
            max_lp_units: 1,
        },
        price_limits: Moe2PriceLimits { max_output_slippage_bps: 0, max_purchase_impact_bps: 2000 },
    }
}

pub(super) async fn exercise(
    env: &mut Env,
    state: &PalwChainStateV2,
    mut book: Moe2SettlementBook,
    model: Hash64,
    remote: Option<Replay>,
    fork: &[Block],
) -> (Observation, Replay) {
    let point = *state.last_point().unwrap();
    let registered = *book.model(&model).unwrap();
    let bond = env.chain.bonds[1];
    let capital = state.bond(&bond).unwrap().collateral;
    let policy = env.bundle.state.bond_budget().unwrap().policy.clone();
    let mut budget = state.bond_budget().unwrap().clone();
    let reward = 160 * SOMPI_PER_KASPA;
    let core = registration_quote(state, env.bundle.state.base_class_id(), bond, policy.digest());
    let allocation = core.preview_at_anchor(&env.config.params, state, reward).unwrap();
    assert_eq!((allocation.miner, allocation.panel_pool, allocation.market), (reward, 0, None));
    let before_profile = Moe2PreDrawQuote { valid_from: 0, ..core };
    assert_eq!(
        before_profile.preview_at_anchor(&env.config.params, state, reward),
        Err(Moe2SettlementError::UnknownClaimContext),
        "even a registered genesis model cannot start MOE2 terms before activation"
    );
    let terms = registration_quote(state, model, bond, policy.digest()).for_claim(Hash64::from_u64_word(641));
    let source = terms.reservation_key();
    let shared_undo = remote.as_ref().map(|r| r.undo.as_slice().to_vec());
    // Peers receive the signed enrollment and both raw blocks. Only the first peer
    // signs. Each reader resolves the verification key from its own native bond.
    const DOMAIN: &[u8] = b"MISAKA/private/MOE2/accounting-enrollment/v1";
    let (enrollment, execution, signature, remote_carrier) = if let Some(replay) = remote {
        let [a, b, c] = replay.blocks.map(|b| Block::try_from(b).unwrap());
        arrive(&env.chain, a.clone(), "MOE2 economic terms").await;
        arrive(&env.chain, b.clone(), "MOE2 accounting position").await;
        (a, b, replay.signature, Some(c))
    } else {
        let key = TestConsensus::palw_v2_registry_keypair(1);
        let sig = ml_dsa_87::sign(&key.signing_key, source.as_byte_slice(), DOMAIN, [44; 32]).unwrap();
        let mut miner = new_miner_data();
        miner.extra_data = source.as_byte_slice().to_vec();
        let interval = env.config.params.target_time_per_block();
        let a = env.chain.heartbeat_paying(miner, interval, vec![]).await;
        let b = env.chain.heartbeat(interval, vec![]).await;
        (a, b, sig.as_slice().to_vec(), None)
    };
    let api = &env.chain.ctx.consensus;
    assert!(api.palw_coinbase_commits_to(env.domain.as_bytes(), enrollment.hash(), &source.as_bytes()).unwrap());
    assert!(api.is_chain_ancestor_of(point.block, enrollment.hash()).unwrap());
    assert!(api.is_chain_ancestor_of(enrollment.hash(), execution.hash()).unwrap());
    assert_ne!(enrollment.hash(), execution.hash());
    let vk = ml_dsa_87::MLDSA87VerificationKey::new(state.bond(&bond).unwrap().pubkey.as_slice().try_into().unwrap());
    let sig = ml_dsa_87::MLDSA87Signature::new(signature.as_slice().try_into().unwrap());
    ml_dsa_87::verify(&vk, source.as_byte_slice(), DOMAIN, &sig).unwrap();
    let (tip, next_state) = env.chain.tip_state();
    assert_eq!(tip, execution.hash());
    assert_eq!(state.model_market(&model), next_state.model_market(&model));
    let at = Moe2Position { point: *next_state.last_point().unwrap(), ordinal: 1 };
    book.admit(terms, |anchor, id| (anchor == point.block && id == model).then_some(registered)).unwrap();
    let mut journal = Vec::new();
    let held = u128::from(reward) * 4;
    let ask = PalwBudgetAskV1 { block_units: 0, reward_sompi: reward, final_weight: 0, liability_sompi: held };
    let grant = budget
        .reserve_claim_v1(&policy, capital, source, bond, Some(model), point.daa_score, ask, PalwBudgetOriginV1::Attempt, &mut journal)
        .unwrap();
    assert_eq!(grant.reward_sompi, reward);
    let before = budget.clone();
    let start = journal.len();
    let row = *budget.claim_row(&source).unwrap();
    assert_eq!(row.model.unwrap().0, model);
    // Only the reservation, market, budget policy and collateral here are read from
    // the native engine. Maturity and the claimant release terms are fixtures.
    let funding = Moe2SettlementFunding {
        claim: terms.claim,
        position: at,
        miner_now: reward * 4 / 10,
        panel_now: reward / 10,
        liability: PalwSettlementTermsSourcedV1 {
            reserved_reward: Ok(row.reserved.reward_sompi),
            consumed_reward: Ok(row.consumed.reward_sompi),
            draw: reward,
            payment: reward,
            paid_now: reward,
            exported_before: Ok(0),
            export_cap: Ok(palw_bond_budget_export_cap_v1(&policy, held)),
            liability_sompi: Ok(row.liability_sompi),
            bond_collateral: Ok(capital.into()),
            other_live_liability: Ok(budget.bond_row(&bond).unwrap().open_liability_sompi - held),
            bond_locked_until: Ok(terms.deadline),
            liability_until: Ok(terms.deadline),
            signed_hard_deadline: Ok(terms.deadline),
            material_ttl_until: Ok(terms.deadline),
            vest_until: None,
            retire_not_before: Ok(terms.deadline),
            now: at.point.daa_score,
        },
    };
    // The unchanged selected tip has the model, the market and the same bond, but
    // no reward enrollment. Reusing its bare claim or a changed quote cannot pay.
    let mut unfunded = state.bond_budget().unwrap().clone();
    let mut unfunded_journal = Vec::new();
    unfunded
        .reserve_claim_v1(
            &policy,
            capital,
            terms.claim,
            bond,
            Some(model),
            point.daa_score,
            ask,
            PalwBudgetOriginV1::Attempt,
            &mut unfunded_journal,
        )
        .unwrap();
    let wrong_terms = Moe2SettlementTerms { deadline: terms.deadline + 1, ..terms };
    assert!(ml_dsa_87::verify(&vk, wrong_terms.reservation_key().as_byte_slice(), DOMAIN, &sig).is_err());
    assert_eq!(
        book.settle(terms.claim, at, point.block, &funding, &mut unfunded, &mut unfunded_journal),
        Err(Moe2SettlementError::ReservationMismatch)
    );
    assert_eq!(unfunded.claim_row(&terms.claim).unwrap().consumed.reward_sompi, 0);
    let mut wrong = funding;
    wrong.liability.draw -= 1;
    assert!(book.settle(terms.claim, at, point.block, &wrong, &mut budget, &mut journal).is_err());
    assert_eq!(budget, before);
    assert_eq!(journal.len(), start);
    assert!(book.receipt(&terms.claim).is_none());
    // A reservation still exists under the exact signed key. A changed active
    // policy must hold that reward even when the monetary fields all agree.
    let mut changed_policy = policy.clone();
    changed_policy.export_cap_permille -= 1;
    let mut changed_budget = budget.clone();
    changed_budget.header.policy_digest = changed_policy.digest();
    let unchanged = (book.clone(), changed_budget.clone());
    assert_eq!(
        book.settle(terms.claim, at, point.block, &funding, &mut changed_budget, &mut journal),
        Err(Moe2SettlementError::ReservationMismatch)
    );
    assert_eq!((&book, &changed_budget, journal.len()), (&unchanged.0, &unchanged.1, start));
    let admitted = book.clone();
    let mut reference = book.clone();
    let (expected_payment, expected_undo) =
        reference.settle_recorded(terms.claim, at, point.block, &funding, &mut budget, &mut journal).unwrap();
    let directory = kaspa_database::utils::get_kaspa_tempdir();
    let store = directory.path();
    let (native, pay, undo) = checkpoint::NativeSettlement::project(
        store,
        env,
        state,
        enrollment.hash(),
        &journal[..start],
        &mut book,
        &terms,
        at,
        Moe2WorkerRelease {
            reward,
            miner_now: funding.miner_now,
            panel_now: funding.panel_now,
            liability_until: terms.deadline,
            signed_hard_deadline: terms.deadline,
            material_ttl_until: terms.deadline,
            vest_until: None,
            retire_not_before: terms.deadline,
        },
    );
    assert_eq!((&book, pay, undo), (&reference, expected_payment, expected_undo));
    let receipt = *book.receipt(&terms.claim).unwrap();
    let (checkpoint, checkpoint_bytes) = book.checkpoint(native.child_root);
    let undo_bytes = borsh::to_vec(&undo).unwrap();
    if let Some(shared) = shared_undo {
        assert_eq!(shared, undo_bytes);
    }

    const BOOK_BYTE_BUDGET: usize = 16 * 1024;
    let record = PrivateMoe2Checkpoint {
        network: env.domain,
        reservation: source,
        enrollment: enrollment.hash(),
        execution: execution.hash(),
        native_root: native.child_root,
        book_root: checkpoint,
    };
    let carrier = if let Some(block) = remote_carrier {
        arrive(&env.chain, block.clone(), "MOE2 settlement checkpoint").await;
        block
    } else {
        let mut miner = new_miner_data();
        miner.extra_data = record.marker().as_byte_slice().to_vec();
        env.chain.heartbeat_paying(miner, env.config.params.target_time_per_block(), vec![]).await
    };
    let api = env.chain.ctx.consensus.as_ref();
    assert_eq!(record.read_selected(api, carrier.hash(), &checkpoint_bytes, BOOK_BYTE_BUDGET).unwrap(), book);
    assert!(record.read_selected(api, execution.hash(), &checkpoint_bytes, BOOK_BYTE_BUDGET).is_err());
    let wrong_record = PrivateMoe2Checkpoint { native_root: native.parent_root, ..record };
    assert!(wrong_record.read_selected(api, carrier.hash(), &checkpoint_bytes, BOOK_BYTE_BUDGET).is_err());
    let mut corrupt = checkpoint_bytes.clone();
    corrupt[0] ^= 1;
    assert!(record.read_selected(api, carrier.hash(), &corrupt, BOOK_BYTE_BUDGET).is_err());

    let obs = Observation {
        enrollment: enrollment.hash(),
        checkpoint,
        carrier: carrier.hash(),
        position: execution.hash(),
        reservation: source,
        consumed: budget.claim_row(&source).unwrap().consumed.reward_sompi,
        reserve: receipt.after.msk_reserve,
        retired: receipt.retired_units,
        miner: pay.miner_now,
        panel: pay.panel_now,
    };
    assert_eq!((receipt.purchase, receipt.liquidity), (48 * SOMPI_PER_KASPA, 32 * SOMPI_PER_KASPA));
    assert_eq!((receipt.purchased_units, receipt.liquidity_units, obs.retired), (2, 1, 1));
    assert_eq!((obs.reserve, obs.miner, obs.panel), (80 * SOMPI_PER_KASPA, 64 * SOMPI_PER_KASPA, 16 * SOMPI_PER_KASPA));
    assert_eq!(obs.consumed, obs.reserve + obs.miner + obs.panel);
    assert!(state.bond_budget().unwrap().claim_row(&source).is_none());
    std::fs::write(store.join("settled"), &checkpoint_bytes).unwrap();
    std::fs::write(store.join("disconnect"), &undo_bytes).unwrap();
    drop(book);
    drop(budget);
    for block in fork {
        arrive(&env.chain, block.clone(), "branch without MOE2 enrollment").await;
    }
    let (fork_tip, _) = env.chain.tip_state();
    for detached in [enrollment.hash(), execution.hash(), carrier.hash()] {
        assert!(!env.chain.ctx.consensus.is_chain_ancestor_of(detached, fork_tip).unwrap());
    }
    let loaded = std::fs::read(store.join("settled")).unwrap();
    // The old block body is still locally available. Withdrawal is decided by
    // selected ancestry rather than deleting the historical commitment.
    assert!(
        env.chain.ctx.consensus.palw_coinbase_commits_to(env.domain.as_bytes(), carrier.hash(), &record.marker().as_bytes()).unwrap()
    );
    assert!(record.read_selected(env.chain.ctx.consensus.as_ref(), carrier.hash(), &loaded, BOOK_BYTE_BUDGET).is_err());
    let mut book = Moe2SettlementBook::from_checkpoint(native.child_root, checkpoint, &loaded, BOOK_BYTE_BUDGET).unwrap();
    assert!(Moe2SettlementBook::from_checkpoint(native.parent_root, checkpoint, &loaded, loaded.len()).is_err());
    assert!(Moe2SettlementBook::from_checkpoint(native.child_root, checkpoint, &loaded, loaded.len() - 1).is_err());
    let restored_undo = borsh::from_slice(&std::fs::read(store.join("disconnect")).unwrap()).unwrap();
    let restored_native = native.reload(env);
    let mut budget = restored_native.bond_budget().unwrap().clone();
    let mut inverse = Vec::new();
    book.revert_settlement(&restored_undo, &mut budget, &mut inverse).unwrap();
    assert_eq!(book, admitted);
    assert_eq!(budget.claim_row(&source), before.claim_row(&source));
    assert_eq!(inverse.len(), 1);
    assert_eq!(book.revert_settlement(&restored_undo, &mut budget, &mut inverse), Err(Moe2SettlementError::UndoMismatch));
    assert_eq!(inverse.len(), 1);
    native.disconnect_and_replay(env, state, &restored_native, &budget, &inverse);
    // Replay only on the archived original branch, with its own position and signed terms.
    let (_, replayed) = book.settle_recorded(terms.claim, at, point.block, &funding, &mut budget, &mut Vec::new()).unwrap();
    assert_eq!(replayed, undo);
    assert_eq!(book.checkpoint(native.child_root), (checkpoint, checkpoint_bytes));
    println!("MOE2 accounting projection pid={} {obs:?} rollback=true", std::process::id());
    println!(
        "MOE2 pid={} detached_enrollment=true native_parent={} native_child={} undo_bytes={} restored_market=true restored_worker_rights=true restored_lp=true replay=true",
        std::process::id(),
        native.parent_root,
        native.child_root,
        undo_bytes.len()
    );
    println!(
        "MOE2 pid={} checkpoint_carrier={} marker={} native_debit_and_market=true detached_checkpoint_rejected=true",
        std::process::id(),
        carrier.hash(),
        record.marker()
    );
    (obs, Replay { blocks: [&enrollment, &execution, &carrier].map(RpcRawBlock::from), signature, undo: undo_bytes })
}

#[path = "moe2_checkpoint.rs"]
mod checkpoint;
