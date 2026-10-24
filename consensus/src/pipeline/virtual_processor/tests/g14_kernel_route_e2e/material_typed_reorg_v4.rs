//! Actual native Typed replacement rollback: stored bytes are not authority,
//! and absence/current-generation change is not permission to garbage collect.
use super::*;
use crate::model::stores::virtual_state::VirtualStateStoreReader;
use kaspa_consensus_core::palw_kernel_material_enrollment_v4::PalwMaterialGenerationAuthorityErrorV4;
use misaka_palw_remote::monetary_material_gc::native_v4::native_material_release_finalized_v4;

#[test]
fn rfc08_typed_replacement_native_reorg_restart_keeps_material_and_reaccepts_identical_carrier() {
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", &child_name("typed_writer_child"), "--nocapture", "--test-threads=1"])
        .env("MISAKA_TYPED_WRITER_CHILD_V4", "1")
        .env("MISAKA_TYPED_REPLACEMENT_CHILD_V4", "1")
        .env("MISAKA_TYPED_REORG_CHILD_V4", "1")
        .status()
        .unwrap();
    assert!(status.success(), "actual Typed replacement reorg role fixture failed: {status}");
}

pub(super) async fn send_saved_carrier(net: &mut Net, carrier: &Transaction) {
    net.mempool(carrier).expect("the actual current UTXO view accepts this exact signed carrier");
    let t = net.ttpb();
    let carrying = net.chain.heartbeat(t, vec![carrier.clone()]).await;
    assert!(carrying.transactions.iter().any(|tx| tx.id() == carrier.id()));
    net.chain.heartbeat(t, vec![]).await;
}

pub(super) fn rolled_back_provider_read(
    store: &mut Provider,
    native: &dyn ConsensusApi,
    input: &ReaderInput,
    provider: PalwBondKeyV2,
    pubkey: &[u8],
    consent: Hash64,
    withdrawn: bool,
) {
    let root = input.request.intent.manifest.root;
    assert_eq!(store.held_contracts(), vec![(consent, root)]);
    let bytes = std::fs::read(store.file()).unwrap();
    let metadata = std::fs::read(store.file().with_extension("kernel-context-v4")).unwrap();
    let scope = input.request.intent.manifest.scope(root).unwrap();
    let authority = native.palw_material_generation_authority_checked_v4(root, consent, provider, scope);
    if withdrawn {
        assert!(
            matches!(authority, Err(PalwMaterialGenerationAuthorityErrorV4::Denied(_))),
            "a rolled-back generation must be denied, not resource/unavailable or historical authority"
        );
        assert!(
            store
                .answer_material_exact_current_v4(
                    consent,
                    root,
                    root,
                    provider,
                    pubkey,
                    PalwMonetaryMaterialDemandV1::Kernel { stage: 0, position: 0 },
                    native,
                )
                .is_err(),
            "persisted replacement bytes do not authorize an answer after native rollback"
        );
    } else {
        authority.expect("initial generation regained actual current authority");
        store
            .answer_material_exact_current_v4(
                consent,
                root,
                root,
                provider,
                pubkey,
                PalwMonetaryMaterialDemandV1::Kernel { stage: 0, position: 0 },
                native,
            )
            .unwrap();
    }
    // This real accepted checkpoint has no Release receipt. The saved tuple is
    // an explicit negative control, never invented positive release evidence.
    assert!(
        !store
            .gc_finalized(root, consent, consent, input.expected_tip, input.expected_tip, |r, c, tail, block| {
                native_material_release_finalized_v4(native, r, c, tail, block)
            },)
            .unwrap(),
        "native absence/rollback alone cannot authorize physical deletion"
    );
    assert_eq!(store.held_contracts(), vec![(consent, root)]);
    assert_eq!(std::fs::read(store.file()).unwrap(), bytes);
    assert_eq!(std::fs::read(store.file().with_extension("kernel-context-v4")).unwrap(), metadata);
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn rollback_and_restore(
    mut w: World,
    mut db: std::sync::Arc<kaspa_database::prelude::DB>,
    dir: &std::path::Path,
    request: &kaspa_consensus_core::palw_prospective_kernel_material_v4::PalwProspectiveKernelMaterialRequestV4,
    initial: Hash64,
    replacement: Hash64,
    fork: Hash64,
    carrier: Transaction,
    original_bytes: &[u8],
) -> (World, std::sync::Arc<kaspa_database::prelude::DB>) {
    let root = request.intent.manifest.root;
    assert_eq!(w.net.chain.tip_state().1.material_recovery_row_v2(root).unwrap().contract().generations.len(), 2);
    let mut branch = t12_genesis_chain(&w.net.config, &w.net.bundle, &w.net.premine, &w.net.floats);
    let prefix = chain_blocks(&w.net.chain, fork);
    branch.ctx.simulated_time = prefix.last().unwrap().header.timestamp;
    for block in prefix {
        arrive(&branch, block, "Typed pre-replacement native prefix").await;
    }
    let t = w.net.ttpb();
    for _ in 0..5 {
        let block = branch.heartbeat(t, vec![]).await;
        arrive(&w.net.chain, block, "Typed actual replacement rollback branch").await;
    }
    assert_eq!(w.net.chain.sink(), branch.sink(), "heavier real native branch became selected");
    let rollback = w.net.chain.tip_state().1;
    assert_eq!(rollback.state_root(), branch.tip_state().1.state_root(), "independent native replay agrees after rollback");
    let row = rollback.monetary_material_row_v1(root).unwrap();
    assert_eq!((row.sequence, row.consent), (0, initial));
    assert_eq!(rollback.material_recovery_row_v2(root).unwrap().contract().generations.len(), 1);
    assert!(w.net.ledger().claims.contains_key(&root.as_bytes()), "fork preserves the actually admitted original claim");
    let rollback_root = rollback.state_root();
    drop(rollback);
    drop(branch);
    assert_eq!(borsh::to_vec(&carrier).unwrap().as_slice(), original_bytes);
    // Cold native restart and physical provider reopen happen WHILE authority
    // for the replacement is withdrawn. Both generations must remain on disk.
    (w, db) = reader_roundtrip(w, db, dir, request, true, true, true);
    assert_eq!(w.net.chain.tip_state().1.state_root(), rollback_root);
    assert_eq!(w.net.chain.tip_state().1.monetary_material_row_v1(root).unwrap().consent, initial);
    let input = carrier.inputs[0].previous_outpoint;
    let change = TransactionOutpoint::new(carrier.id(), 0);
    let utxos = Net::utxos_of(&w.net.chain);
    let input_live = utxos.contains_key(&input);
    let change_live = utxos.contains_key(&change);
    let virtual_accepted = w.net.chain.vp().virtual_stores.read().state.get().unwrap().accepted_tx_ids.contains(&carrier.id());
    let chain_accepted = chain_blocks(&w.net.chain, w.net.chain.sink()).iter().any(|b| {
        w.net
            .chain
            .ctx
            .consensus
            .get_block_acceptance_data(b.header.hash)
            .unwrap()
            .iter()
            .any(|m| m.accepted_transactions.iter().any(|tx| tx.transaction_id == carrier.id()))
    });
    eprintln!(
        "TYPED_REORG_V4 input_live={input_live} change_live={change_live} virtual_accepted={virtual_accepted} chain_accepted={chain_accepted}"
    );
    if input_live {
        assert!(!change_live && !virtual_accepted && !chain_accepted);
        send_saved_carrier(&mut w.net, &carrier).await;
    } else {
        assert!(
            change_live && virtual_accepted && !chain_accepted,
            "unexpected actual carrier UTXO/acceptance clocks; do not rebuild funding or infer native authority"
        );
        let block = w.net.chain.heartbeat(t, vec![]).await;
        assert!(
            w.net
                .chain
                .ctx
                .consensus
                .get_block_acceptance_data(block.header.hash)
                .unwrap()
                .iter()
                .any(|m| m.accepted_transactions.iter().any(|tx| tx.transaction_id == carrier.id())),
            "actual heartbeat must accept the original mixed-blue signed transaction"
        );
    }
    assert_eq!(borsh::to_vec(&carrier).unwrap().as_slice(), original_bytes, "reorg never rebuilds or re-funds the carrier");
    let row = w.net.chain.tip_state().1.monetary_material_row_v1(root).unwrap();
    assert_eq!((row.sequence, row.consent), (1, replacement), "exact saved signed generation must be natively restored");
    assert_eq!(w.net.chain.tip_state().1.material_recovery_row_v2(root).unwrap().contract().generations.len(), 2);
    let restored = w.net.chain.tip_state().1.state_root();
    (w, db) = reader_roundtrip(w, db, dir, request, true, true, false);
    assert_eq!(w.net.chain.tip_state().1.state_root(), restored);
    (w, db)
}
