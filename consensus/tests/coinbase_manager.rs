//! Run the actual coinbase manager's unit controls without compiling the
//! virtual processor's entire unit-test module. This does not test block
//! acceptance, PALW native settlement, or selected-verifier approval.
pub use kaspa_consensus::{model, params};

#[path = "../src/constants/mod.rs"]
mod constants;

#[path = "../src/processes/coinbase.rs"]
mod coinbase_manager;

#[test]
fn palw_queue_coinbase_prefix_is_consumed_once_and_carriage_delta_restore_it_exactly() {
    use coinbase_manager::{CoinbaseManager, palw_pending_payout_outputs_v2};
    use kaspa_consensus::model::stores::ghostdag::GhostdagData;
    use kaspa_consensus_core::{
        BlockHashMap, BlockHashSet, Hash64,
        blockhash::BlockHashes,
        coinbase::{BlockRewardData, MinerData},
        mldsa87_primitives::p2pkh_mldsa87_spk,
        palw_state_v2::{
            PALW_V2_MAX_PAYOUTS_PER_BLOCK, PalwBlockContextV2, PalwChainStateV2, PalwPayoutV2, PalwStateCarriageV2, PalwStateParamsV2,
            apply_delta_v2, apply_palw_transition_v2, revert_delta_v2,
        },
    };
    // Controlled queue fixture. This tests rendering/drain/replay, not the
    // earning, funding or selected-validator approval of any reward leg.
    let params = PalwStateParamsV2::new(100, 1, 1, 1, 500, 1_000, Hash64::from_u64_word(1), 4, 1_000, 100, 1_000, 0).unwrap();
    let mut carriage = PalwStateCarriageV2::from_state(&PalwChainStateV2::genesis());
    let count = PALW_V2_MAX_PAYOUTS_PER_BLOCK + 2;
    for index in (0..count).rev() {
        let mut key = [0; 64];
        key[..8].copy_from_slice(&(index as u64).to_be_bytes());
        carriage.pending_payouts.insert(
            Hash64::from_bytes(key),
            PalwPayoutV2 { payload: Hash64::from_u64_word(100 + index as u64), amount: 1 + index as u64 },
        );
    }
    let parent = carriage.into_state(&params, None).unwrap();
    let expected_total: u64 = (1..=count as u64).sum();
    let network = &params::MAINNET_PARAMS;
    let manager = CoinbaseManager::new(
        network.coinbase_payload_script_public_key_max_len,
        network.max_coinbase_payload_len,
        network.deflationary_phase_daa_score,
        network.pre_deflationary_phase_base_subsidy,
        network.target_time_per_block_history(),
    );
    let selected = Hash64::from_u64_word(900);
    let script = p2pkh_mldsa87_spk(&[7; 64]);
    let mut rewards = BlockHashMap::default();
    rewards.insert(selected, BlockRewardData::new(0, 0, 0, script.clone()));
    let ghostdag =
        GhostdagData::new(1, 0.into(), selected, BlockHashes::new(vec![selected]), BlockHashes::new(vec![]), Default::default());
    let build = |state: &PalwChainStateV2| {
        let outputs = palw_pending_payout_outputs_v2(state);
        let coinbase = manager
            .expected_coinbase_transaction(
                1,
                0,
                MinerData::new(script.clone(), vec![]),
                &ghostdag,
                &rewards,
                &BlockHashSet::default(),
                &outputs,
                None,
                (0, 0),
                0,
                &BlockHashSet::default(),
                false,
                &BlockHashMap::default(),
                &BlockHashSet::default(),
                &BlockHashMap::default(),
            )
            .unwrap();
        assert_eq!(coinbase.tx.outputs, outputs);
        coinbase.tx
    };
    let first = build(&parent);
    assert_eq!(first.outputs.len(), PALW_V2_MAX_PAYOUTS_PER_BLOCK);
    let restored = PalwStateCarriageV2::from_state(&parent).into_state(&params, Some(parent.state_root())).unwrap();
    assert_eq!(build(&restored), first);
    let point = |index| PalwBlockContextV2 { block: Hash64::from_u64_word(index), daa_score: index, blue_score: index, subsidy: 0 };
    let (child, delta) = apply_palw_transition_v2(&restored, &params, &point(1), &[], None).unwrap();
    assert_eq!(child.pending_payouts_iter().count(), 2);
    let second = build(&child);
    assert_eq!(second.outputs.len(), 2);
    for (index, output) in first.outputs.iter().chain(&second.outputs).enumerate() {
        assert_eq!(output.value, 1 + index as u64);
        assert_eq!(output.script_public_key, p2pkh_mldsa87_spk(&Hash64::from_u64_word(100 + index as u64).as_bytes()));
    }
    assert_eq!(first.outputs.iter().chain(&second.outputs).map(|o| o.value).sum::<u64>(), expected_total);
    let (empty, _) = apply_palw_transition_v2(&child, &params, &point(2), &[], None).unwrap();
    assert!(build(&empty).outputs.is_empty());
    assert_eq!(apply_delta_v2(&parent, &delta, &params).unwrap(), child);
    let rolled_back = revert_delta_v2(&child, &delta, &params).unwrap();
    assert_eq!(rolled_back, parent);
    assert_eq!(build(&rolled_back), first);
}
