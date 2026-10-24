//! Signed native recovery generations and the exact provider GC intent consumer.
//! Legacy KernelPositions is explicit here; this does not accept TypedKernelV4
//! prospective work or establish a K3/provider campaign.
use super::*;
use crate::model::stores::virtual_state::VirtualStateStoreReader;
use kaspa_consensus_core::{
    api::ConsensusApi,
    palw_material_capacity_v1::{PalwKernelMaterialBundleV1, PalwMaterialManifestV1},
    palw_material_release_recovery_v2::{
        PalwMaterialRecoveryActionV2 as R, PalwMaterialRecoveryRowV2 as RR, PalwMaterialRecoverySignatureV2 as RS,
        PalwMaterialRecoveryTermsV2 as RT, PalwMaterialReleaseRecoveryFenceV2, material_recovery_object_v2,
    },
    palw_monetary_material_v1::{
        PalwMonetaryMaterialActionV1 as M, PalwMonetaryMaterialEnrollmentV1 as Enrollment, PalwMonetaryMaterialFenceV1,
        PalwMonetaryMaterialOfferV1 as Offer, PalwMonetaryMaterialPolicyV1, PalwMonetaryMaterialReplacementV1 as Replacement,
        PalwMonetaryMaterialTermsV1 as Terms,
    },
    palw_provider_court_v1::{PALW_PROVIDER_COURT_MLDSA87_CONTEXT_V1 as DOMAIN, palw_provider_court_message_v1},
};
use misaka_palw_remote::{
    monetary_material::DurableMonetaryMaterialProviderV1 as Provider,
    monetary_material_gc::{
        MonetaryMaterialGcOracleV1, RemoteFinalityAuthorityV1, RemoteProofGcOracleV2, native_v4::native_material_release_finalized_v4,
    },
    monetary_material_rpc::{MonetaryMaterialProofPagerV1, ProvenMonetarySnapshotV2},
};

const MATERIAL_FROM: u64 = 30;

pub(super) fn config() -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    let (c, bundle, premine, floats) = kernel_config_opv(opv_admitted());
    let mut p = c.params.clone();
    p.palw_consensus_mode = kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle);
    p.palw_provider_court_v1 = Some(ForkActivation::new(MATERIAL_FROM));
    p.palw_reporter_income_v2 = Some(ForkActivation::new(MATERIAL_FROM));
    p.palw_material_income_v3 = Some(ForkActivation::new(MATERIAL_FROM));
    let window = p.palw_bond_budget_v1.as_ref().unwrap().policy.window_daa;
    p.palw_monetary_material_v1 = Some(PalwMonetaryMaterialFenceV1 {
        activation: ForkActivation::new(MATERIAL_FROM),
        policy: PalwMonetaryMaterialPolicyV1 {
            version: 1,
            max_contracts: 4,
            max_contracts_per_provider: 4,
            max_bytes_per_contract: 4 << 20,
            max_bytes_per_provider: 16 << 20,
            minimum_collateral_sompi: 1,
            minimum_fee_sompi: 1,
            window_daa: window,
            response_window_daa: 20,
            max_enrollment_daa: 100,
            cost_report_root: Hash64::from_bytes([0xA8; 64]),
        },
    });
    p.palw_material_release_recovery_v2 =
        Some(PalwMaterialReleaseRecoveryFenceV2::unapproved_probe_at(ForkActivation::new(MATERIAL_FROM)));
    // An explicit fixture finality depth, not a public network recommendation.
    p.blockrate.finality_depth = 20;
    p.sync_palw_bond_budget_v1();
    assert!(p.validate_palw_monetary_material_v1().is_err());
    assert!(p.validate_palw_material_release_recovery_v2().is_err());
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(b) = &p.palw_consensus_mode else { unreachable!() };
    (Config::new(p.clone()), b.clone(), premine, floats)
}
fn sign(net: &Net, bond: &PalwBondKeyV2, message: &[u8], domain: &[u8]) -> Option<Vec<u8>> {
    let card = net.chain.bonds.iter().position(|b| b == bond)?;
    let key = TestConsensus::palw_v2_registry_keypair(card as u64);
    libcrux_ml_dsa::ml_dsa_87::sign(&key.signing_key, message, domain, [0xA8; 32]).ok().map(|s| s.as_ref().to_vec())
}
fn recovery(net: &Net, action: R) -> Obj {
    material_recovery_object_v2(network(net), action, net.bond(1), |b, m, d| sign(net, b, m, d)).unwrap()
}
fn network(net: &Net) -> Hash64 {
    Hash64::from_bytes(net.chain.tip_state().1.kernel_route().unwrap().header.policy.network_domain)
}
fn signatures(net: &Net, cards: [usize; 2]) -> Vec<RS> {
    let mut providers: Vec<_> = cards.into_iter().map(|c| RS { provider: net.bond(c), signature: vec![] }).collect();
    providers.sort_by_key(|p| p.provider);
    providers
}
fn snapshot(native: &dyn ConsensusApi, pin: Hash64) -> Result<ProvenMonetarySnapshotV2, String> {
    let page = native.palw_monetary_aux_page_v1(pin, 0, 0, 1 << 20)?;
    let mut pager = MonetaryMaterialProofPagerV1::new(pin, None, &page, 4 << 20, 32 << 20, 4)?;
    pager.push(&page)?;
    while pager.cursor().0 < page.total_rows {
        let (row, offset) = pager.cursor();
        let page = native.palw_monetary_aux_page_v1(pin, row, offset, 1 << 20)?;
        pager.push(&page)?;
    }
    pager.finish_with_recovery_v2()
}
struct NativePin<'a> {
    native: &'a dyn ConsensusApi,
    pin: Hash64,
}
impl RemoteFinalityAuthorityV1 for NativePin<'_> {
    fn finalized_pin(&mut self) -> Result<Hash64, String> {
        Ok(self.pin)
    }
    fn selected_parent(&mut self, block: Hash64) -> Result<Hash64, String> {
        self.native.get_ghostdag_data(block).map(|g| g.selected_parent).map_err(|e| e.to_string())
    }
    fn is_chain_ancestor_of(&mut self, low: Hash64, high: Hash64) -> Result<bool, String> {
        if low == high {
            return Ok(true);
        }
        let path = self.native.get_virtual_chain_from_block(low, None).map_err(|e| e.to_string())?;
        Ok(path.removed.is_empty() && path.added.contains(&high))
    }
    fn chain_after(&mut self, low: Hash64, limit: usize) -> Result<Vec<Hash64>, String> {
        self.native.get_virtual_chain_from_block(low, Some(limit)).map(|p| p.added).map_err(|e| e.to_string())
    }
}

/// Reuse the exact signed/funded carrier after a reorg. The old containing
/// block may remain in the DAG; selected-chain acceptance is the authority.
async fn send_saved_carrier(net: &mut Net, tx: &Transaction) {
    net.mempool(tx).expect("current UTXO view admits the exact saved carrier");
    let t = net.ttpb();
    let carrying = net.chain.heartbeat(t, vec![tx.clone()]).await;
    assert!(carrying.transactions.iter().any(|candidate| candidate.id() == tx.id()));
    net.chain.heartbeat(t, vec![]).await;
}

#[tokio::test]
async fn rfc08_signed_replacement_native_terminal_binds_exact_gc_release_after_restart_and_pruned_delta() {
    use kaspa_database::{create_temp_db, prelude::ConnBuilder};
    kaspa_core::log::try_init_logger("warn");
    let (_life, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let (tx, rx) = async_channel::unbounded();
    let mut net = Net::over_cfg(config(), |c| TestConsensus::with_db(db.clone(), c, tx));
    net._keep.push(Box::new(rx));
    let mut w = World::on_opv(net).await;
    let job = w.job().await;
    let claim = w.claim(0, &job, false).await;
    assert!(
        w.net.ledger().claims[&claim.id].committed_daa < MATERIAL_FROM,
        "legacy KernelPositions authenticates a real historical pre-fence claim"
    );
    w.net.beat_to(MATERIAL_FROM).await;
    let current = w.net.chain.tip_state().1;
    assert!(w.net.config.params.palw_provider_court_v1.unwrap().is_active(w.net.daa()));
    assert!(w.net.bundle.state.monetary_material_at_v1(w.net.daa()).is_some());
    assert!(w.net.bundle.state.material_release_recovery_at_v2(w.net.daa()).is_some());
    assert!(current.kernel_route().unwrap().ledger().unwrap().claims.contains_key(&claim.id));
    let ledger = w.net.ledger();
    let positions = PalwKernelMaterialBundleV1 {
        claim: Hash64::from_bytes(claim.id),
        positions: (0..claim.trace.values.len()).map(|p| (0, p as u32, claim.position(&w.fx, p as u32, |_| {}))).collect(),
    };
    let scope = positions.verify(&ledger, 4 << 20).unwrap();
    let root = Hash64::from_bytes([0xB1; 64]);
    let policy = w.net.config.params.palw_monetary_material_v1.as_ref().unwrap().policy.clone();
    let rp = w.net.config.params.palw_material_release_recovery_v2.as_ref().unwrap().policy.clone();
    let terms = Terms {
        version: 1,
        root,
        dependencies: vec![positions.claim],
        exec_plan_root: None,
        sponsor: w.net.bond(1),
        nonce: Hash64::from_bytes([0xB2; 64]),
        policy: policy.id(),
        max_material_bytes: 4 << 20,
        material_manifest: Some(PalwMaterialManifestV1 { root, scopes: vec![scope] }),
        native_session: None,
        window_daa: policy.window_daa,
        enrollment_until_daa: w.net.daa() + 80,
        payment_relative_until_finality: true,
    };
    let enrollment = |cards: [usize; 2]| {
        let mut offers: Vec<_> = cards
            .into_iter()
            .map(|c| Offer { provider: w.net.bond(c), collateral_sompi: 7, prepaid_fee_sompi: 5_000_007, signature: vec![] })
            .collect();
        offers.sort_by_key(|o| o.provider);
        Enrollment { terms: terms.clone(), offers }
    };
    let initial = enrollment([2, 3]);
    let next = enrollment([4, 5]);
    let initial_consent = initial.id();
    // Actual material enters a durable provider before any service signatures.
    let dir = std::env::temp_dir().join(format!("misaka-native-gc-{}-{}", std::process::id(), root));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("material");
    let mut store = Provider::open(&file, 4 << 20, 16 << 20, Some(&ledger), |_| None).unwrap();
    assert_eq!(
        store.prepare_material(w.net.bond(2), &initial, &[], std::slice::from_ref(&positions), &[], Some(&ledger), |_| None).unwrap(),
        initial_consent
    );
    let action = R::FundedEnroll {
        monetary: Box::new(initial.clone()),
        terms: RT {
            version: 2,
            root,
            initial_consent,
            sponsor: w.net.bond(1),
            nonce: terms.nonce,
            policy: rp.id(),
            max_generations: 4,
            all_generation_ack: true,
        },
        providers: signatures(&w.net, [2, 3]),
        sponsor_signature: vec![],
    };
    let object = recovery(&w.net, action);
    w.net.send(vec![(1, object)]).await;
    assert!(w.net.chain.ctx.consensus.palw_kernel_material_consent_accepted_v4(root, initial_consent).unwrap());
    let fork = w.net.chain.sink();
    let recovery_id = w.net.chain.tip_state().1.material_recovery_row_v2(root).unwrap().contract().id();
    let replacement = Replacement { root, previous: initial_consent, sequence: 1, enrollment: next };
    let action = R::ReplaceBeforeV4 {
        valid_until_daa: w.net.daa() + 60,
        recovery: recovery_id,
        replacement: Box::new(replacement),
        providers: signatures(&w.net, [4, 5]),
        sponsor_signature: vec![],
    };
    let final_consent = action.bounded_replacement_consent_v4().unwrap();
    assert!(!w.net.chain.ctx.consensus.palw_kernel_material_consent_accepted_v4(root, final_consent).unwrap());
    let object = recovery(&w.net, action);
    let saved_carrier = w.net.carrier(1, &object);
    let saved_bytes = borsh::to_vec(&saved_carrier).unwrap();
    send_saved_carrier(&mut w.net, &saved_carrier).await;
    let native: &dyn ConsensusApi = w.net.chain.ctx.consensus.as_ref();
    assert!(native.palw_kernel_material_consent_accepted_v4(root, final_consent).unwrap());
    let state = w.net.chain.tip_state().1;
    assert_eq!(state.monetary_material_row_v1(root).unwrap().consent, final_consent);
    assert_eq!(state.material_recovery_row_v2(root).unwrap().contract().generations.len(), 2);
    // A heavier branch from before replacement withdraws current authority.
    // Neither durable provider bytes nor saved signatures imply acceptance.
    let mut branch = t12_genesis_chain(&w.net.config, &w.net.bundle, &w.net.premine, &w.net.floats);
    let prefix = chain_blocks(&w.net.chain, fork);
    branch.ctx.simulated_time = prefix.last().unwrap().header.timestamp;
    for block in prefix {
        arrive(&branch, block, "pre-replacement native prefix").await;
    }
    let t = w.net.ttpb();
    for _ in 0..5 {
        let block = branch.heartbeat(t, vec![]).await;
        arrive(&w.net.chain, block, "replacement rollback branch").await;
    }
    assert_eq!(w.net.chain.sink(), branch.sink());
    assert!(!w.net.chain.ctx.consensus.palw_kernel_material_consent_accepted_v4(root, final_consent).unwrap());
    assert_eq!(w.net.chain.tip_state().1.material_recovery_row_v2(root).unwrap().contract().generations.len(), 1);
    assert_eq!(store.held_contracts(), vec![(initial_consent, root)]);
    assert_eq!(borsh::to_vec(&saved_carrier).unwrap(), saved_bytes, "rollback never rebuilds signed funding bytes");
    let input = saved_carrier.inputs[0].previous_outpoint;
    let change = TransactionOutpoint::new(saved_carrier.id(), 0);
    let current_utxos = Net::utxos_of(&w.net.chain);
    let input_live = current_utxos.contains_key(&input);
    let change_live = current_utxos.contains_key(&change);
    let virtual_accepted = w.net.chain.vp().virtual_stores.read().state.get().unwrap().accepted_tx_ids.contains(&saved_carrier.id());
    let chain_accepted = chain_blocks(&w.net.chain, w.net.chain.sink()).iter().any(|b| {
        w.net
            .chain
            .ctx
            .consensus
            .get_block_acceptance_data(b.header.hash)
            .unwrap()
            .iter()
            .any(|m| m.accepted_transactions.iter().any(|t| t.transaction_id == saved_carrier.id()))
    });
    eprintln!(
        "[replacement-reorg] input_live={input_live} change_live={change_live} virtual_accepted={virtual_accepted} chain_accepted={chain_accepted}"
    );
    if input_live {
        assert!(!change_live && !virtual_accepted && !chain_accepted, "a rolled-back ordinary spend cannot also remain accepted");
        send_saved_carrier(&mut w.net, &saved_carrier).await;
    } else {
        // A selected-parent PALW rollback does not imply the current virtual
        // UTXO view forgot a mixed-blue carrier. Commit that real virtual
        // acceptance in one actual block; never spend its consumed input again.
        assert!(
            change_live && virtual_accepted && !chain_accepted,
            "unexpected carrier view: input_live={input_live}, change_live={change_live}, virtual_accepted={virtual_accepted}, chain_accepted={chain_accepted}"
        );
        let block = w.net.chain.heartbeat(t, vec![]).await;
        let accepted = w.net.chain.ctx.consensus.get_block_acceptance_data(block.header.hash).unwrap();
        assert!(
            accepted.iter().any(|m| m.accepted_transactions.iter().any(|tx| tx.transaction_id == saved_carrier.id())),
            "the actual block must accept the saved mixed-blue carrier"
        );
    }
    assert!(
        w.net.chain.ctx.consensus.palw_kernel_material_consent_accepted_v4(root, final_consent).unwrap(),
        "the next actual fold must apply the exact accepted carrier generation"
    );
    let accepted = w.net.chain.tip_state().1.state_root();
    drop(store);
    w = w.restart(db.clone());
    assert_eq!(w.net.chain.tip_state().1.state_root(), accepted);
    let ledger = w.net.ledger();
    let mut store = Provider::open(&file, 4 << 20, 16 << 20, Some(&ledger), |_| None).unwrap();
    assert_eq!(store.held_contracts(), vec![(initial_consent, root)]);
    // No finalized terminal authority exists yet, even though the replacement
    // was accepted. Retained old generations remain physical obligations.
    assert!(!native_material_release_finalized_v4(w.net.chain.ctx.consensus.as_ref(), root, initial_consent, root, fork).unwrap());
    let deadline = w.net.daa() + 1500;
    let tail = loop {
        let row = w.net.chain.tip_state().1.monetary_material_row_v1(root).unwrap();
        if let Some(tail) = row.tail_block {
            break tail;
        }
        assert!(w.net.daa() < deadline, "actual fee/payment liability must eventually yield a tail");
        let t = w.net.ttpb();
        w.net.chain.heartbeat(t, vec![]).await;
    };
    // Native finality/ancestry admission remains mandatory; wait for its own
    // rehearsal, rather than installing a prefilled terminal receipt.
    let release = loop {
        let action = M::Release { root, consent: final_consent, tail_block: tail };
        let payload = borsh::to_vec(&action).unwrap();
        let signer = w.net.bond(1);
        let message = palw_provider_court_message_v1(network(&w.net), 242, &signer, &payload);
        let object = Obj::MonetaryMaterialActionV1 {
            action: Box::new(action),
            signer,
            signature: sign(&w.net, &signer, message.as_byte_slice(), DOMAIN).unwrap(),
        };
        if matches!(
            w.net.chain.ctx.consensus.palw_object_rehearsal_v1(&object),
            Some(kaspa_consensus_core::palw_producer_v2::PalwObjectRehearsalV1::Accepted)
        ) {
            break object;
        }
        assert!(w.net.daa() < deadline, "release must become natively admissible after its real tail");
        let t = w.net.ttpb();
        w.net.chain.heartbeat(t, vec![]).await;
    };
    w.net.send(vec![(1, release)]).await;
    let terminal = match w.net.chain.tip_state().1.material_recovery_row_v2(root).unwrap() {
        RR::Terminal(r) => r,
        _ => panic!("actual Release writes terminal"),
    };
    assert_eq!(terminal.final_consent, final_consent);
    let (pin, receipt) = loop {
        if let Some(receipt) = w.net.chain.ctx.consensus.palw_material_terminal_receipt_finalized_v2(root, initial_consent).unwrap() {
            break receipt;
        }
        assert!(w.net.daa() < deadline);
        let t = w.net.ttpb();
        w.net.chain.heartbeat(t, vec![]).await;
    };
    assert_eq!(receipt, terminal);
    for consent in [initial_consent, final_consent] {
        let native: &dyn ConsensusApi = w.net.chain.ctx.consensus.as_ref();
        assert!(native_material_release_finalized_v4(native, root, consent, tail, receipt.release_block).unwrap());
        assert!(!native_material_release_finalized_v4(native, root, consent, tail, fork).unwrap());
        let mut remote = RemoteProofGcOracleV2 { authority: NativePin { native, pin }, proven_snapshot: |b| snapshot(native, b) };
        assert!(remote.release_finalized(root, consent, tail, receipt.release_block).unwrap());
        assert!(!remote.release_finalized(root, consent, tail, fork).unwrap());
        assert!(!remote.release_finalized(root, consent, fork, receipt.release_block).unwrap());
    }
    let native: &dyn ConsensusApi = w.net.chain.ctx.consensus.as_ref();
    assert!(
        !store
            .gc_finalized(root, initial_consent, initial_consent, tail, fork, |r, c, t, b| native_material_release_finalized_v4(
                native, r, c, t, b
            ))
            .unwrap()
    );
    assert_eq!(store.held_contracts(), vec![(initial_consent, root)]);
    // Controlled history removal: this is not a pruned-IBD campaign. The
    // finalized positive receipt must avoid reading the old Release delta.
    w.net.chain.vp().palw_state_v2_store.write().delete_delta_for_tests(receipt.release_block).unwrap();
    assert!(native_material_release_finalized_v4(native, root, initial_consent, tail, receipt.release_block).unwrap());
    assert!(
        store
            .gc_finalized(root, initial_consent, initial_consent, tail, receipt.release_block, |r, c, t, b| {
                native_material_release_finalized_v4(native, r, c, t, b)
            })
            .unwrap()
    );
    assert!(store.held_contracts().is_empty());
    drop(store);
    let store = Provider::open(&file, 4 << 20, 16 << 20, Some(&w.net.ledger()), |_| None).unwrap();
    assert!(store.held_contracts().is_empty());
    drop(store);
    std::fs::remove_dir_all(dir).unwrap();
}
