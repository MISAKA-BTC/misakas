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
            PALW_V2_MAX_PAYOUTS_PER_BLOCK, PalwBlockContextV2, PalwChainStateV2, PalwStateCarriageV2, apply_delta_v2,
            apply_palw_transition_v2, revert_delta_v2,
        },
    };
    // Controlled queue fixture. This tests rendering/drain/replay, not the
    // earning, funding or selected-validator approval of any reward leg.
    let count = PALW_V2_MAX_PAYOUTS_PER_BLOCK + 2;
    let (params, parent) = payout_fixture(count);
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
        let outputs = palw_pending_payout_outputs_v2(state).unwrap();
        let coinbase = manager
            .expected_coinbase_transaction(
                1,
                0,
                MinerData::new(script.clone(), vec![]),
                &ghostdag,
                &rewards,
                &BlockHashSet::default(),
                outputs.iter(),
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
        assert_eq!(coinbase.tx.outputs.as_slice(), outputs.as_ref());
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

fn payout_fixture(
    count: usize,
) -> (kaspa_consensus_core::palw_state_v2::PalwStateParamsV2, kaspa_consensus_core::palw_state_v2::PalwChainStateV2) {
    use kaspa_consensus_core::{
        Hash64,
        palw_state_v2::{PalwChainStateV2, PalwPayoutV2, PalwStateCarriageV2, PalwStateParamsV2},
    };
    let params = PalwStateParamsV2::new(100, 1, 1, 1, 500, 1_000, Hash64::from_u64_word(1), 4, 1_000, 100, 1_000, 0).unwrap();
    let mut carriage = PalwStateCarriageV2::from_state(&PalwChainStateV2::genesis());
    for index in (0..count).rev() {
        let mut key = [0; 64];
        key[..8].copy_from_slice(&(index as u64).to_be_bytes());
        carriage.pending_payouts.insert(
            Hash64::from_bytes(key),
            PalwPayoutV2 { payload: Hash64::from_u64_word(100 + index as u64), amount: 1 + index as u64 },
        );
    }
    let state = carriage.into_state(&params, None).unwrap();
    (params, state)
}

#[test]
fn palw_payout_native_capacity_child() {
    use coinbase_manager::{palw_pending_payout_outputs_v2, palw_pending_payout_requests_v2};
    use kaspa_consensus_core::palw_native_resource_v1::{
        NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1, reserve_native_requests_v1,
    };
    let Ok(case) = std::env::var("MISAKA_PALW_PAYOUT_CAPACITY_CASE") else {
        return;
    };
    // Native admission is process-global: each case runs in its own child.
    // This quota covers the render buffer, not the pre-existing fixture state.
    let count = if case == "empty" {
        0
    } else if case.ends_with("prefix") {
        kaspa_consensus_core::palw_state_v2::PALW_V2_MAX_PAYOUTS_PER_BLOCK + 2
    } else {
        1
    };
    let (_, state) = payout_fixture(count);
    let root = state.state_root();
    let request = palw_pending_payout_requests_v2(&state);
    let memory = if case == "zero" {
        0
    } else if case.starts_with("below") {
        request - 1
    } else {
        request
    };
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: memory, database_bytes: 0 }));
    if case == "zero" || case.starts_with("below") {
        assert!(palw_pending_payout_outputs_v2(&state).is_err());
        assert!(reserve_native_requests_v1(memory).is_ok(), "a failed render must release every reservation");
    } else {
        let outputs = palw_pending_payout_outputs_v2(&state).unwrap();
        if case == "empty" {
            assert!(outputs.is_empty());
            assert_eq!(request, 0);
        } else {
            assert_eq!(outputs.len(), count.min(kaspa_consensus_core::palw_state_v2::PALW_V2_MAX_PAYOUTS_PER_BLOCK));
            assert!(outputs.iter().all(|output| output.script_public_key.script().len() == 69));
            assert!(reserve_native_requests_v1(1).is_err(), "the live output owner retains its full request");
        }
        drop(outputs);
        assert!(reserve_native_requests_v1(memory).is_ok(), "dropping the output owner returns capacity");
    }
    assert_eq!(state.state_root(), root);
}

#[test]
fn palw_payout_render_rejects_short_capacity_and_holds_its_owner_until_drop() {
    for case in ["zero", "below", "exact", "below-prefix", "exact-prefix", "empty"] {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "palw_payout_native_capacity_child", "--test-threads=1"])
            .env("MISAKA_PALW_PAYOUT_CAPACITY_CASE", case)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "capacity case {case}: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[path = "coinbase_manager/allocation_counter.rs"]
mod allocation_counter;

struct BorrowedCoinbaseFixture {
    manager: coinbase_manager::CoinbaseManager,
    ghostdag: model::stores::ghostdag::GhostdagData,
    rewards: kaspa_consensus_core::BlockHashMap<kaspa_consensus_core::coinbase::BlockRewardData>,
    non_daa: kaspa_consensus_core::BlockHashSet,
    unentitled: kaspa_consensus_core::BlockHashSet,
    withheld: kaspa_consensus_core::BlockHashMap<u64>,
    rounds: kaspa_consensus_core::BlockHashSet,
    receipts: kaspa_consensus_core::BlockHashMap<kaspa_consensus_core::palw_receipt_v4::PalwReceiptV4Payout>,
    extra: Vec<kaspa_consensus_core::tx::TransactionOutput>,
    miner_script: kaspa_consensus_core::tx::ScriptPublicKey,
    carve: Option<kaspa_consensus_core::dns_finality::FeeSplitParams>,
    pay_reds: bool,
    own_subsidy: u64,
    escrow: u64,
}
impl BorrowedCoinbaseFixture {
    fn new(extra: usize) -> Self {
        use kaspa_consensus_core::{Hash64, blockhash::BlockHashes, coinbase::BlockRewardData, mldsa87_primitives::p2pkh_mldsa87_spk};
        let network = &params::MAINNET_PARAMS;
        let manager = coinbase_manager::CoinbaseManager::new(
            network.coinbase_payload_script_public_key_max_len,
            network.max_coinbase_payload_len,
            network.deflationary_phase_daa_score,
            network.pre_deflationary_phase_base_subsidy,
            network.target_time_per_block_history(),
        );
        let miner_script = p2pkh_mldsa87_spk(&[7; 64]);
        let mut rewards = kaspa_consensus_core::BlockHashMap::default();
        rewards.insert(Hash64::from_u64_word(1), BlockRewardData::new(0, 0, 0, miner_script.clone()));
        Self {
            manager,
            ghostdag: model::stores::ghostdag::GhostdagData::new(
                1,
                0.into(),
                Hash64::from_u64_word(1),
                BlockHashes::new(vec![Hash64::from_u64_word(1)]),
                BlockHashes::new(vec![]),
                Default::default(),
            ),
            rewards,
            non_daa: Default::default(),
            unentitled: Default::default(),
            withheld: Default::default(),
            rounds: Default::default(),
            receipts: Default::default(),
            extra: (0..extra)
                .map(|n| {
                    kaspa_consensus_core::tx::TransactionOutput::new(
                        10 + n as u64,
                        p2pkh_mldsa87_spk(&Hash64::from_u64_word(100 + n as u64).as_bytes()),
                    )
                })
                .collect(),
            miner_script,
            carve: None,
            pay_reds: false,
            own_subsidy: 1000,
            escrow: 0,
        }
    }
    fn build(&self) -> kaspa_consensus_core::tx::Transaction {
        self.manager
            .expected_coinbase_transaction(
                1,
                self.own_subsidy,
                kaspa_consensus_core::coinbase::MinerData::new(self.miner_script.clone(), &[]),
                &self.ghostdag,
                &self.rewards,
                &self.non_daa,
                self.extra.iter(),
                self.carve.as_ref(),
                (1, 1),
                self.escrow,
                &self.unentitled,
                self.pay_reds,
                &self.withheld,
                &self.rounds,
                &self.receipts,
            )
            .unwrap()
            .tx
    }
    fn compare(&self, actual: &kaspa_consensus_core::tx::Transaction) -> coinbase_manager::CoinbaseComparisonV2 {
        self.compare_with_miner(actual, kaspa_consensus_core::coinbase::MinerData::new(self.miner_script.clone(), &[])).unwrap()
    }
    fn compare_with_miner<'a>(
        &self,
        actual: &kaspa_consensus_core::tx::Transaction,
        miner: kaspa_consensus_core::coinbase::MinerData<&'a [u8]>,
    ) -> Result<coinbase_manager::CoinbaseComparisonV2,
        kaspa_consensus_core::palw_native_resource_v1::NativeDecodeErrorV1<kaspa_consensus_core::errors::coinbase::CoinbaseError>> {
        self.manager
            .compare_coinbase_transaction_v2(
                actual,
                1,
                self.own_subsidy,
                miner,
                &self.ghostdag,
                &self.rewards,
                &self.non_daa,
                self.extra.iter(),
                self.carve.as_ref(),
                (1, 1),
                self.escrow,
                &self.unentitled,
                self.pay_reds,
                &self.withheld,
                &self.rounds,
                &self.receipts,
            )
    }
}

#[test]
fn borrowed_coinbase_comparison_covers_every_hashed_field_and_output_order() {
    use kaspa_consensus_core::{
        Hash64, hashing, subnets,
        tx::{TransactionInput, TransactionOutpoint, TransactionOutput},
    };
    let fixture = BorrowedCoinbaseFixture::new(8);
    let expected = fixture.build();
    assert!(fixture.compare(&expected).matches);
    for mutation in 0..11 {
        let mut actual = expected.clone();
        match mutation {
            0 => actual.version += 1,
            1 => actual.inputs.push(TransactionInput::new(TransactionOutpoint::new(Hash64::from_u64_word(99), 0), vec![], 0, 0)),
            2 => {
                actual.outputs.pop();
            }
            3 => actual.outputs.push(TransactionOutput::new(1, fixture.miner_script.clone())),
            4 => actual.outputs.swap(0, 1),
            5 => actual.outputs[0].value += 1,
            6 => actual.outputs[0].script_public_key = fixture.miner_script.clone(),
            7 => actual.lock_time = 1,
            8 => actual.subnetwork_id = subnets::SUBNETWORK_ID_NATIVE,
            9 => actual.gas = 1,
            10 => actual.set_mass(1),
            _ => unreachable!(),
        }
        let comparison = fixture.compare(&actual);
        assert_eq!(comparison.matches, hashing::tx::hash(&actual) == hashing::tx::hash(&expected), "field {mutation}");
        assert!(!comparison.matches, "field {mutation}");
    }
    let mut changed_payload = expected.clone();
    changed_payload.payload[0] ^= 1;
    let comparison = fixture.compare(&changed_payload);
    assert!(!comparison.matches);
    assert!(!comparison.payload_matches);
    assert!(comparison.first_output_difference.is_none());
    let mut changed_output = expected;
    changed_output.outputs[3].value += 1;
    assert_eq!(fixture.compare(&changed_output).first_output_difference, Some((3, false, true, 14, 13)));
}

#[test]
fn borrowed_coinbase_comparison_shares_blue_red_round_receipt_and_carve_rules() {
    use kaspa_consensus_core::{
        Hash64, blockhash::BlockHashes, coinbase::BlockRewardData, dns_finality::FeeSplitParams, palw_receipt_v4::PalwReceiptV4Payout,
    };
    let h = Hash64::from_u64_word;
    let mut fixture = BorrowedCoinbaseFixture::new(8);
    fixture.ghostdag.mergeset_reds = BlockHashes::new(vec![h(2), h(3), h(4)]);
    fixture.rewards.insert(h(1), BlockRewardData::new(1000, 100, 40, fixture.miner_script.clone()));
    fixture.rewards.insert(h(2), BlockRewardData::new(1000, 90, 30, fixture.miner_script.clone()));
    fixture.rewards.insert(h(3), BlockRewardData::new(0, 70, 0, fixture.extra[0].script_public_key.clone()));
    fixture.rewards.insert(h(4), BlockRewardData::new(0, 80, 0, fixture.extra[0].script_public_key.clone()));
    fixture.rounds.extend([h(3), h(4)]);
    fixture.non_daa.extend([h(3), h(4)]);
    fixture.receipts.insert(h(1), PalwReceiptV4Payout { miner_script: fixture.extra[1].script_public_key.clone(), fee_bps: 500 });
    let split = FeeSplitParams {
        subsidy_worker_base_bps: 6200,
        subsidy_worker_inclusion_bps: 800,
        subsidy_validator_bps: 3000,
        subsidy_service_bps: 0,
        normal_fee_worker_bps: 9000,
        normal_fee_validator_bps: 1000,
        normal_fee_service_bps: 0,
        finality_fee_validator_bps: 7500,
        finality_fee_worker_bps: 2500,
        finality_fee_service_bps: 0,
    };
    for carve in [None, Some(split)] {
        fixture.carve = carve;
        for pay_reds in [false, true] {
            fixture.pay_reds = pay_reds;
            for deny_red in [false, true] {
                fixture.unentitled.clear();
                if deny_red {
                    fixture.unentitled.insert(h(2));
                }
                for (subsidy, escrow) in [(0, 0), (1000, 200)] {
                    fixture.own_subsidy = subsidy;
                    fixture.escrow = escrow;
                    fixture.withheld.clear();
                    if escrow > 0 {
                        fixture.withheld.insert(h(2), 100);
                    }
                    let expected = fixture.build();
                    let comparison = fixture.compare(&expected);
                    assert!(comparison.matches);
                    assert_eq!(comparison.expected_outputs, expected.outputs.len());
                    assert!(
                        expected
                            .outputs
                            .iter()
                            .any(|out| out.value == 150 && out.script_public_key == fixture.extra[0].script_public_key)
                    );
                }
            }
        }
    }
}

#[test]
fn borrowed_validation_allocation_requests_do_not_grow_with_the_eight_payout_scripts() {
    let mut observations = Vec::new();
    for count in [0, 1, 8] {
        let fixture = BorrowedCoinbaseFixture::new(count);
        let expected = fixture.build();
        assert!(fixture.compare(&expected).matches); // warm one-time runtime machinery before observing requests
        // Input construction is outside the observation: this measures the
        // actual comparison body, not the caller's owned MinerData clone.
        let miner = kaspa_consensus_core::coinbase::MinerData::new(fixture.miner_script.clone(), &[][..]);
        let (comparison, requested) = allocation_counter::requested(|| fixture.compare_with_miner(&expected, miner).unwrap());
        assert!(comparison.matches);
        observations.push(requested);
        println!("borrowed coinbase comparison: payouts={count}, requested_bytes={requested}");
    }
    assert_eq!(observations, vec![0, 0, 0], "non-round comparison has no temporary heap allocations");
    assert_eq!(observations[0], observations[1]);
    assert_eq!(observations[0], observations[2], "comparison must not allocate owning payout scripts or output rows");
}

#[test]
fn coinbase_round_comparison_capacity_child() {
    if std::env::var("MISAKA_COINBASE_ROUND_CAPACITY").ok().as_deref() != Some("1") {
        return;
    }
    use kaspa_consensus_core::{
        Hash64, blockhash::BlockHashes, coinbase::{BlockRewardData, MinerData},
        palw_native_resource_v1::{NativeDecodeErrorV1, NativeSnapshotResourceLimitsV1,
            arm_native_logical_limits_v1, reserve_native_requests_v1},
    };
    let mut fixture = BorrowedCoinbaseFixture::new(8);
    let round = Hash64::from_u64_word(2);
    fixture.rounds.insert(round);
    fixture.ghostdag.mergeset_reds = BlockHashes::new(vec![round]);
    fixture.rewards.insert(round, BlockRewardData::new(0, 25, 0, fixture.miner_script.clone()));
    let expected = fixture.build();
    let miner = MinerData::new(fixture.miner_script.clone(), &[][..]);
    let memory = 1 << 20;
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: memory, database_bytes: 0 }));
    let blocker = reserve_native_requests_v1(memory).unwrap();
    let result = fixture.compare_with_miner(&expected, miner);
    assert!(matches!(result, Err(NativeDecodeErrorV1::Resource(_))), "local quota is not a bad-coinbase verdict");
    drop(blocker);
    assert!(fixture.compare(&expected).matches);
    assert!(reserve_native_requests_v1(memory).is_ok(), "comparison scratch released on return");
    let mut bad = expected;
    bad.outputs[0].value += 1;
    assert!(!fixture.compare(&bad).matches);
    assert!(reserve_native_requests_v1(memory).is_ok(), "scratch released on mismatch as well");
}

#[test]
fn round_comparison_retries_after_native_capacity_refusal() {
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "coinbase_round_comparison_capacity_child", "--nocapture"])
        .env("MISAKA_COINBASE_ROUND_CAPACITY", "1").output().unwrap();
    assert!(out.status.success(), "{} {}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(String::from_utf8_lossy(&out.stdout).contains("1 passed"));
}
