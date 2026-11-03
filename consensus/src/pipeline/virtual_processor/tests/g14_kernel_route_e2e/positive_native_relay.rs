//! Private mechanics through real UTXO acceptance and coinbase validation.
use super::*;
use crate::model::stores::{acceptance_data::AcceptanceDataStoreReader, virtual_state::VirtualStateStoreReader};
use kaspa_consensus_core::{
    config::params::{PositiveAssignmentFenceV2, PositiveCourtFenceV2},
    palw_native_resource_v1::{NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1},
    palw_positive_relay_signature_v2::FundingRelaySignatureTemplateV2,
    palw_positive_rows_v2::PositiveRowsV2,
    subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
};
use kaspa_database::prelude::{ConnBuilder, DB};
use misaka_palw_positive_contract::{
    assignment::{
        AssignmentId,
        funding_relay::*,
        funding_relay_rows::{FundingRelayAuthorizationRowV2, FundingRelayCustodyRowV2},
    },
    identity::{Digest as ContractDigest, principal},
    native::NativeRowKey,
    rows::{NativeHash64, NativeOutpoint, NativeRowBody},
};
use std::{
    path::Path,
    sync::{Arc, Weak},
};

#[path = "../../../../../core/tests/support/positive_assignment.rs"]
mod assignment_fixture;
#[path = "../../../../../core/tests/support/funding_relay.rs"]
mod relay_fixture;

const RELAY_CHILD: &str = "MISAKA_TEST_FUNDED_NATIVE_RELAY_CHILD";
const GROSS_FEE: u64 = 100_000_000;
const RELAY_ACTIVATION_DAA: u64 = 8;

#[test]
fn positive_initial_relay_burns_once_through_the_native_node() {
    let name = format!("{}::native_relay_worker", module_path!().split_once("::").unwrap().1);
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", &name, "--nocapture", "--test-threads=1"])
        .env(RELAY_CHILD, "1")
        .status()
        .unwrap();
    assert!(status.success(), "the private native relay fixture failed: {status}");
}

#[inline(never)]
fn relay_network(make: impl FnOnce(&Config) -> TestConsensus) -> Net {
    let (config, bundle, premine, floats) = kernel_config();
    let mut params = config.params;
    // The current template preview covers relay-only acceptance outside the
    // timestamp-dependent execution lane and DNS-retirement overlay.
    params.palw_execution_lane = None;
    params.palw_dns_retirement = None;
    let activation = ForkActivation::new(RELAY_ACTIVATION_DAA);
    let positive = PositiveCourtFenceV2::unapproved_probe_at(activation);
    params.palw_positive_assignment_v2 =
        Some(PositiveAssignmentFenceV2 { activation, policy: assignment_fixture::policy(&positive), native_preimages: None });
    params.palw_positive_court_v2 = Some(positive);
    params.palw_positive_funding_relay_v2 = Some(relay_fixture::relay_fence(activation.daa_score()));
    assert!(params.validate_palw_positive_funding_relay_v2().is_err());
    Net::over_cfg((Config::new(params), bundle, premine, floats), make)
}

fn signed_relay(net: &Net) -> FundingRelayRegistrationBodyV2 {
    let params = &net.config.params;
    let daa = net.daa();
    let scope = params.palw_positive_assignment_rule_scope_v2_at(daa).unwrap();
    let relay_scope = params.palw_positive_funding_relay_rule_scope_v2_at(daa).unwrap();
    let fence = params.palw_positive_funding_relay_v2_at(daa).unwrap();
    let keypair = TestConsensus::palw_v2_registry_keypair(0);
    let key = *keypair.verification_key.as_ref();
    let payer_bond = net.bond(0).0;
    let funding_outpoint = NativeOutpoint { transaction: NativeHash64([0x42; 64]), index: 1 };
    let terms = FundingRelayTermsV2 {
        version: 2,
        revision: 1,
        contract_network: ContractDigest(net.domain.as_bytes()),
        contract_ruleset: ContractDigest(scope.ruleset().as_bytes()),
        assignment: AssignmentId(ContractDigest([0x53; 64])),
        contract_anchor: NativeHash64(net.chain.sink().as_bytes()),
        relay_ruleset: relay_scope.ruleset(),
        relay_policy: fence.policy.id(),
        payer_bond: NativeOutpoint { transaction: NativeHash64(payer_bond.transaction_id.as_bytes()), index: payer_bond.index },
        payer: principal(&key),
        refund_payload: ContractDigest(kaspa_hashes::blake2b_512_address_payload(&key).as_bytes()),
        funding_purpose: RelayFundingPurposeV2::PosterA,
        target: RelayTargetV2::InitialAssignment { bundle: ContractDigest([0x64; 64]), poster_nonce: 0 },
        funding_outpoint,
        expected_amount: 1000,
        template: RelayTemplateV2::NativeOp12NoCaseNoReceiptsV1,
        register_before_daa: daa + 60,
        funding_admission_before_daa: daa + 120,
        custody_until_daa: daa + 300,
        metadata_fee_schedule: fence.fees.id(),
        physical_custody_profile: fence.physical.id(),
        max_metadata_fee: GROSS_FEE,
    };
    let template = FundingRelaySignatureTemplateV2::new(
        terms.assignment,
        TransactionOutpoint::new(Hash64::from_bytes(funding_outpoint.transaction.0), funding_outpoint.index),
        terms.expected_amount,
        key,
    )
    .unwrap();
    let purpose =
        libcrux_ml_dsa::ml_dsa_87::sign(&keypair.signing_key, &terms.id().0, FUNDING_RELAY_REGISTER_CONTEXT_V2, [0x11; 32]).unwrap();
    FundingRelayRegistrationBodyV2 {
        version: 2,
        revision: 1,
        operation: 14,
        terms,
        purpose_signature: *purpose.as_ref(),
        relay_signature: template.sign(&keypair.signing_key, [0x22; 32]).unwrap(),
    }
}

fn carrier(net: &Net, card: usize, payload: &[u8], fee: u64) -> Transaction {
    let (outpoint, entry) = net.funding[card].clone();
    let mut tx = Transaction::new(
        crate::constants::TX_VERSION,
        vec![TransactionInput::new(outpoint, vec![], 0, 1)],
        vec![TransactionOutput::new(entry.amount - fee, card_payout_spk(card))],
        0,
        SUBNETWORK_ID_PALW_LIFECYCLE,
        0,
        payload.to_vec(),
    );
    sign_spend(&mut tx, entry, card, net.config.params.storage_mass_parameter);
    tx
}

#[inline(never)]
fn reopen_relay_net(
    net: Net,
    path: &Path,
    previous: &mut Weak<DB>,
    carrier: &Transaction,
    rooted: &(FundingRelayAuthorizationRowV2, FundingRelayCustodyRowV2),
    payload: &[u8],
) -> Net {
    let sink = net.chain.sink();
    let root = root_at(&net.chain, sink);
    let spent = carrier.inputs[0].previous_outpoint;
    let change = TransactionOutpoint::new(carrier.id(), 0);
    let node = &net.chain.ctx.consensus;
    assert!(node.get_virtual_utxo_entry(spent).is_none());
    let change_entry = node.get_virtual_utxo_entry(change).expect("accepted relay change UTXO");
    let net = net.restart_with(|| {
        assert!(previous.upgrade().is_none(), "the old relay database must be closed");
        let db = ConnBuilder::default()
            .with_files_limit(10)
            .with_db_path(path.to_path_buf())
            .with_create_if_missing(false)
            .build()
            .expect("reopen the existing relay database");
        *previous = Arc::downgrade(&db);
        db
    });
    assert_eq!(net.chain.sink(), sink);
    assert_eq!(root_at(&net.chain, sink), root);
    let node = &net.chain.ctx.consensus;
    assert!(node.get_virtual_utxo_entry(spent).is_none());
    assert_eq!(node.get_virtual_utxo_entry(change), Some(change_entry));
    let (auth, custody) = rooted;
    let vp = net.chain.vp();
    let store = vp.palw_state_v2_store.read();
    let (tip, state) = store.load_tip_retained_v1(&net.bundle.state).unwrap().unwrap();
    assert_eq!(tip, sink);
    let rows = PositiveRowsV2::new(&state);
    let funding = auth.terms.funding_outpoint;
    let purpose = auth.terms.funding_purpose;
    let restored_auth = rows.read(NativeRowKey::FundingRelayAuthorization { funding, purpose }).unwrap().unwrap();
    let restored_custody = rows.read(NativeRowKey::FundingRelayCustody { funding, purpose }).unwrap().unwrap();
    assert_eq!(restored_auth.body, NativeRowBody::FundingRelayAuthorization(auth.clone()));
    assert_eq!(restored_custody.body, NativeRowBody::FundingRelayCustody(custody.clone()));
    let fees = store.check_funding_relay_archive_v2(auth, |raw| raw == payload).unwrap();
    assert_eq!(fees.gross_fee(), GROSS_FEE);
    assert_eq!(fees.relay_charge(), auth.quote.fee);
    drop(store);
    drop(vp);
    net
}

#[tokio::test]
async fn native_relay_worker() {
    if std::env::var(RELAY_CHILD).as_deref() == Ok("1") {
        run_relay().await;
    }
}

#[inline(never)]
fn run_relay() -> TestFuture<()> {
    Box::pin(async {
        // Register the class below the fence, then arm the local demand meter
        // before advancing into the active Assignment prefix.
        let dir = kaspa_database::utils::get_kaspa_tempdir();
        let db = ConnBuilder::default().with_files_limit(10).with_db_path(dir.path().to_path_buf()).build().unwrap();
        let mut previous = Arc::downgrade(&db);
        let (sender, receiver) = async_channel::unbounded();
        let mut initial = relay_network(|config| TestConsensus::with_db(db, config, sender));
        initial._keep.push(Box::new(receiver));
        let World { mut net, .. } = World::on(initial).await;
        assert!(net.daa() < RELAY_ACTIVATION_DAA);
        let follower = Box::new(relay_network(TestConsensus::new));
        let limit = 16 << 30;
        arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: limit, database_bytes: limit }));
        net.beat_to(RELAY_ACTIVATION_DAA + 2).await;
        let step = net.ttpb();
        let body = signed_relay(&net);
        let mut payload = vec![1, 0, 176];
        payload.extend(borsh::to_vec(&body).unwrap());
        let fence = net.config.params.palw_positive_funding_relay_v2_at(net.daa()).unwrap();
        let metadata_fee = fence.fees.registration_quote(&fence.policy, &fence.physical, &body.terms, net.daa()).unwrap().fee;
        let short = carrier(&net, 3, &payload, metadata_fee - 1);
        let first = carrier(&net, 0, &payload, GROSS_FEE);
        let retry = carrier(&net, 2, &payload, GROSS_FEE);
        assert_ne!(first.id(), retry.id());
        let outpoint = body.terms.funding_outpoint;
        let purpose = body.terms.funding_purpose;
        let mut rooted = None;
        for (register, gross, tx) in [(false, metadata_fee - 1, short), (true, GROSS_FEE, first), (false, GROSS_FEE, retry)] {
            net.mempool(&tx).expect("ordinary mempool validation");
            let carrying = net.chain.heartbeat(step, vec![tx.clone()]).await;
            assert!(carrying.transactions.iter().any(|candidate| candidate.id() == tx.id()));
            let (quote, tip) = {
                let vp = net.chain.vp();
                let view = vp.virtual_stores.read().state.get().unwrap();
                let fence = net.config.params.palw_positive_funding_relay_v2_at(view.daa_score).unwrap();
                let quote = fence.fees.registration_quote(&fence.policy, &fence.physical, &body.terms, view.daa_score).unwrap();
                (quote, view.mergeset_rewards[&carrying.hash()].total_fees)
            };
            assert_eq!(quote.fee, metadata_fee);
            assert_eq!(tip, if register { gross - quote.fee } else { gross });
            let accepting = net.chain.heartbeat(step, Vec::new()).await;
            let vp = net.chain.vp();
            let acceptance = vp.acceptance_data_store.get(accepting.hash()).unwrap();
            assert!(acceptance.iter().any(|data| data.block_hash == carrying.hash()
                && data.accepted_transactions.iter().any(|accepted| accepted.transaction_id == tx.id())));
            let store = vp.palw_state_v2_store.read();
            let (tip, state) = store.load_tip_retained_v1(&net.bundle.state).unwrap().unwrap();
            assert_eq!(tip, accepting.hash());
            let rows = PositiveRowsV2::new(&state);
            let auth = rows.read(NativeRowKey::FundingRelayAuthorization { funding: outpoint, purpose }).unwrap();
            let custody = rows.read(NativeRowKey::FundingRelayCustody { funding: outpoint, purpose }).unwrap();
            if gross < metadata_fee {
                assert!(auth.is_none() && custody.is_none(), "one sompi short cannot register");
                continue;
            }
            let NativeRowBody::FundingRelayAuthorization(auth) = auth.expect("authorization row").body else { panic!("row kind") };
            let NativeRowBody::FundingRelayCustody(custody) = custody.expect("custody row").body else { panic!("row kind") };
            custody.check_pair(&auth).unwrap();
            assert!(custody.binding.is_none());
            let fees = store.check_funding_relay_archive_v2(&auth, |raw| raw == payload).unwrap();
            assert_eq!(fees.before_relay(), GROSS_FEE);
            if !register {
                assert_eq!(Some((auth, custody)), rooted);
            } else {
                assert_eq!(auth.origin.accepting_block, NativeHash64(accepting.hash().as_bytes()));
                assert_eq!(auth.origin.carrier_transaction, NativeHash64(tx.id().as_bytes()));
                assert_eq!(auth.quote, quote);
                assert_eq!(fees.relay_charge(), quote.fee);
                rooted = Some((auth, custody));
                drop(state);
                drop(store);
                drop(vp);
                net = reopen_relay_net(net, dir.path(), &mut previous, &tx, rooted.as_ref().unwrap(), &payload);
            }
        }

        // A different database validates the blocks, without producing the
        // paying template or sharing the producer's relay custody rows.
        let tip = net.chain.sink();
        for block in chain_blocks(&net.chain, tip) {
            arrive(&follower.chain, block, "native relay peer replay").await;
        }
        assert_eq!(follower.chain.sink(), tip);
        assert_eq!(root_at(&follower.chain, tip), root_at(&net.chain, tip));
        let (auth, _) = rooted.expect("the original registration was accepted");
        let follower_fees =
            follower.chain.vp().palw_state_v2_store.read().check_funding_relay_archive_v2(&auth, |raw| raw == payload).unwrap();
        assert_eq!(follower_fees.relay_charge(), auth.quote.fee);
        drop(net);
        assert!(previous.upgrade().is_none(), "the last relay connection is also closed");
    })
}
