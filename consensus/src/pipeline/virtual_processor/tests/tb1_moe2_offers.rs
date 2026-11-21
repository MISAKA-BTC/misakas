//! Independently signed economic terms fixed before the same EXEC disclosure.
use super::*;
use kaspa_consensus_core::{
    palw_bond_budget_v1::{PALW_CAPITAL_ASSIGNMENT_MLDSA87_CONTEXT_V1, palw_capital_assignment_message_v1},
    palw_model_market_v1::moe2::settlement::Moe2PreDrawQuote,
    palw_state_v2::{PalwChainStateV2, PalwConsensusObjectV2, revert_delta_v2},
    tx::{TransactionOutpoint, UtxoEntry},
};
use libcrux_ml_dsa::ml_dsa_87;
use misaka_palw_remote::tb1_exposure::exec_input::{
    ExecInputTerms,
    moe2::{ClosedMoe2Quote, QUOTE_DOMAIN, consent_ref},
};

pub(super) struct Signatures {
    owner: ml_dsa_65::MLDSA65Signature,
    funder: Option<ml_dsa_87::MLDSA87Signature>,
    reference: [u8; 64],
}

impl Signatures {
    pub(super) fn read_or_sign(
        source: Option<&Path>,
        directory: &Path,
        owner: &ml_dsa_65::MLDSA65KeyPair,
        key: [u8; 64],
        slot: u64,
        quoted: bool,
    ) -> Self {
        let file = source.unwrap_or(directory).join(format!("qf1-{slot}.sig"));
        let signature = if source.is_some() {
            ml_dsa_65::MLDSA65Signature::new(std::fs::read(file).unwrap().as_slice().try_into().unwrap())
        } else {
            let sig = ml_dsa_65::sign(&owner.signing_key, &key, REGISTRATION_DOMAIN, [85; 32]).unwrap();
            std::fs::write(file, sig.as_slice()).unwrap();
            sig
        };
        if !quoted {
            return Self { owner: signature, funder: None, reference: [24; 64] };
        }
        let reference = consent_ref(key, slot);
        let file = source.unwrap_or(directory).join(format!("funding-{slot}.sig"));
        let funder = if source.is_some() {
            ml_dsa_87::MLDSA87Signature::new(std::fs::read(file).unwrap().as_slice().try_into().unwrap())
        } else {
            let bond = crate::consensus::test_consensus::TestConsensus::palw_v2_registry_keypair(1);
            let sig = ml_dsa_87::sign(&bond.signing_key, &reference, QUOTE_DOMAIN, [49; 32]).unwrap();
            std::fs::write(file, sig.as_slice()).unwrap();
            sig
        };
        Self { owner: signature, funder: Some(funder), reference }
    }

    pub(super) fn entry<'a>(
        &'a self,
        terms: &'a ExecInputTerms<'a>,
        owner: &'a ml_dsa_65::MLDSA65VerificationKey,
        slot: u64,
    ) -> SignedExecInput<'a> {
        SignedExecInput {
            terms,
            owner,
            slot,
            signature: &self.owner,
            funding_ref: self.reference,
            quote_consent: self.funder.as_ref(),
        }
    }
}

pub(super) async fn assign(node: &mut Rig, quote: Moe2PreDrawQuote, funds: &mut (TransactionOutpoint, UtxoEntry)) -> [RpcRawBlock; 2] {
    let parent = node.chain.sink();
    let bond = quote.funding_bond;
    assert_eq!(bond, node.chain.bonds[1]);
    let policy = &node.bundle.state.bond_budget().unwrap().policy;
    let rate = kaspa_consensus_core::palw_bond_budget_v1::palw_bond_budget_export_cap_v1(policy, 1000);
    // One new model assignment covers the full export exposure of both offers.
    let amount = u64::try_from(2 * (u128::from(quote.limits.max_reward) * 1000).div_ceil(rate.into())).unwrap();
    let assignments = vec![(quote.model, amount)];
    let msg = palw_capital_assignment_message_v1(node.domain, &bond, &assignments, 1);
    let signer = crate::consensus::test_consensus::TestConsensus::palw_v2_registry_keypair(1);
    let signature = ml_dsa_87::sign(&signer.signing_key, msg.as_byte_slice(), PALW_CAPITAL_ASSIGNMENT_MLDSA87_CONTEXT_V1, [113; 32])
        .unwrap()
        .as_slice()
        .to_vec();
    super::super::super::t12_round_lane_e2e::send_lifecycle(
        &mut node.chain,
        funds,
        PalwConsensusObjectV2::BondCapitalAssignedV1 { bond, assignments, sequence: 1, signature },
    )
    .await;
    let api = node.api().as_ref();
    let folded = node.chain.sink();
    let carried = api.get_ghostdag_data(folded).unwrap().selected_parent;
    assert_eq!(api.get_ghostdag_data(carried).unwrap().selected_parent, parent);
    [carried, folded].map(|h| RpcRawBlock::from(&api.get_block(h).unwrap()))
}

pub(super) fn observe(
    node: &Rig,
    original: &PalwChainStateV2,
    short: &ClosedMoe2Quote,
    long: &ClosedMoe2Quote,
    assignment: Hash64,
    panel_release: u64,
    long_source: Hash64,
) {
    let (tip, state) = node.chain.tip_state();
    let api = node.api().as_ref();
    let (_, delta) = node.vp().palw_state_v2_store.read().delta_of(tip).unwrap();
    let parent = revert_delta_v2(&state, &delta, &node.bundle.state).unwrap();
    let config = &node.config.params;
    let terms = long.terms();
    let now = node.chain.daa_of(tip);
    assert!(short.terms().deadline < panel_release && panel_release <= now && now <= terms.deadline);
    let reward = terms.limits.max_reward;
    assert_eq!(
        short.preview_for_selected_tip(api, config, original, &parent, reward).unwrap_err().kind(),
        std::io::ErrorKind::TimedOut
    );
    assert!(long.preview_for_selected_tip(api, config, &parent, original, reward).is_err());
    assert!(long.preview_for_selected_tip(api, config, original, &state, reward).is_err());
    assert!(long.preview_for_selected_tip(api, config, original, &parent, reward + 1).is_err());
    let split = long.preview_for_selected_tip(api, config, original, &parent, reward).unwrap();
    assert_eq!(split, long.preview(config, original, reward).unwrap());
    let budget = parent.bond_budget().unwrap();
    assert!(budget.claim_row(&long_source).is_none());
    let recorded = budget.assignment_row(&terms.funding_bond).unwrap();
    assert_eq!(recorded.sequence, 1);
    assert!(api.is_chain_ancestor_of(assignment, parent.last_point().unwrap().block).unwrap());
    let pending = recorded.pending.as_ref().unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].0, terms.model);
    assert!(recorded.effective.is_empty());
    let market = split.market.unwrap();
    peer_log!(
        "MOE2 live offer pid={} slot=10 consent={} execution_parent={} tip={} root={} now={} panel_release={} deadline={} assignment={} assignment_daa={} assigned={} buy={} lp={} burned={} miner={} panel={} short_expired=true native_reward_debit=0",
        std::process::id(),
        long_source,
        parent.last_point().unwrap().block,
        tip,
        parent.state_root(),
        now,
        panel_release,
        terms.deadline,
        assignment,
        node.chain.daa_of(assignment),
        pending[0].1,
        market.split.purchase,
        market.split.liquidity,
        market.retired_units,
        split.miner,
        split.panel_pool
    );
}

/// Follow the accepted funding assignment into the first non-bootstrap epoch.
/// Only the producer builds blocks. Each reader validates those raw blocks on
/// its own database and computes the stop condition from its own budget table.
pub(super) async fn season(
    node: &mut Rig,
    socket: &mut Option<UnixStream>,
    peers: &mut [(Child, UnixStream)],
    original: &PalwChainStateV2,
    long: &ClosedMoe2Quote,
    offer: misaka_palw_remote::tb1_exposure::exec_input::moe2::Moe2BudgetPreview,
) {
    use kaspa_consensus_core::palw_bond_budget_v1::{PalwBudgetAskV1, PalwBudgetOriginV1};
    let terms = long.terms();
    let (_, start) = node.chain.tip_state();
    let pending = start.bond_budget().unwrap().assignment_row(&terms.funding_bond).unwrap();
    let amount = pending.pending.as_ref().unwrap()[0].1;
    let (from, policy) = node.bundle.state.bond_budget().unwrap().allocation.clone().unwrap();
    let average_epochs = policy.average_epochs().unwrap();
    let epoch = pending.pending_since_epoch + 1 + u64::from(policy.seasoning_epochs);
    let mature = from + epoch * policy.epoch_daa;
    let end = mature + policy.epoch_daa - 2;
    assert!(end < terms.deadline);
    let count = advance(node, socket, peers, mature, end, (terms.model, offer.reward_sompi)).await;
    let (parent, state) = node.chain.tip_state();
    let committed = local_carrier(node, socket, peers, 0, [0x45; 64], "commit the seasoned model allocation").await;
    assert_eq!(node.api().get_ghostdag_data(committed.hash()).unwrap().selected_parent, parent);
    assert_eq!(committed.header.palw_state_root, state.state_root());
    let budget = state.bond_budget().unwrap();
    assert!(!budget.allocation_bootstrapping_v1());
    let e = budget.header.allocation.unwrap();
    let row = budget.model_row(&terms.model).unwrap();
    let assignment = budget.assignment_row(&terms.funding_bond).unwrap();
    assert_eq!((e.index, e.start_daa, row.epoch), (epoch, mature, epoch));
    assert_eq!(assignment.effective.as_slice(), &[(terms.model, amount)]);
    assert!(assignment.pending.is_none());
    assert_eq!(row.capital, u128::from(amount) / u128::from(average_epochs));
    assert!(budget.claim_row(&offer.consent).is_none());
    assert_eq!(row.reserved_sompi, 0);
    let split = long.preview_for_selected_tip(node.api().as_ref(), &node.config.params, original, &state, offer.reward_sompi).unwrap();
    assert_eq!(split, long.preview(&node.config.params, original, offer.reward_sompi).unwrap());
    let ask = PalwBudgetAskV1 {
        block_units: 0,
        final_weight: 0,
        reward_sompi: offer.reward_sompi,
        liability_sompi: offer.minimum_liability_sompi,
    };
    let mut scratch = budget.clone();
    let grant = scratch
        .reserve_claim_v1(
            &node.bundle.state.bond_budget().unwrap().policy,
            state.bond(&terms.funding_bond).unwrap().collateral,
            offer.consent,
            terms.funding_bond,
            Some(terms.model),
            committed.header.daa_score,
            ask,
            PalwBudgetOriginV1::Attempt,
            &mut Vec::new(),
        )
        .unwrap();
    assert_eq!(grant.reward_sompi, offer.reward_sompi);
    let available = budget.model_available(&terms.model);
    assert!(available >= grant.reward_sompi);
    assert_eq!(scratch.model_available(&terms.model), available - grant.reward_sompi);
    peer_log!(
        "MOE2 seasoned pid={} consent={} parent={} tip={} root={} blocks={} daa={} epoch={} start={} assigned={} avg_epochs={} average={} accrued={} available={} projected_grant={} reserved={} native_reward_debit=0",
        std::process::id(),
        offer.consent,
        parent,
        committed.hash(),
        state.state_root(),
        count,
        committed.header.daa_score,
        epoch,
        mature,
        amount,
        average_epochs,
        row.capital,
        e.accrued_sompi,
        available,
        grant.reward_sompi,
        row.reserved_sompi
    );
}

async fn advance(
    node: &mut Rig,
    socket: &mut Option<UnixStream>,
    peers: &mut [(Child, UnixStream)],
    mature: u64,
    end: u64,
    reward: (Hash64, u64),
) -> usize {
    let step = node.config.params.target_time_per_block();
    let mut count = 0;
    loop {
        let daa = node.chain.daa_of(node.chain.sink());
        if daa >= end || (daa >= mature && node.chain.tip_state().1.bond_budget().unwrap().model_available(&reward.0) >= reward.1) {
            break;
        }
        if let Some(reader) = socket {
            let batch: Vec<RpcRawBlock> = receive(reader);
            assert!((1..=32).contains(&batch.len()));
            for raw in batch {
                let b = Block::try_from(raw).unwrap();
                let hash = b.hash();
                let parent = node.chain.sink();
                node.chain.ctx.simulated_time = node.chain.ctx.simulated_time.max(b.header.timestamp);
                stable_panel::arm(node);
                let result = node.api().validate_and_insert_block(b).virtual_state_task.await.unwrap();
                assert!(result.has_block_body());
                assert_eq!(node.chain.sink(), hash);
                assert_eq!(node.api().get_ghostdag_data(hash).unwrap().selected_parent, parent);
                count += 1;
            }
        } else {
            let mut batch = Vec::with_capacity(32);
            for _ in 0..32 {
                let b = node.chain.heartbeat(step, vec![]).await;
                let daa = b.header.daa_score;
                batch.push(RpcRawBlock::from(&b));
                count += 1;
                if daa >= end || daa == mature {
                    break;
                }
            }
            for (_, peer) in peers.iter_mut() {
                send(peer, &batch);
            }
        }
    }
    count
}
