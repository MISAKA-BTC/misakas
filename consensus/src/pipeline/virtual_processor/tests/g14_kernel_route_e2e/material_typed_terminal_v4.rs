//! Real Typed terminal publication, native finality, and exact janitor WAL replay.
//! No manual delta deletion, pruning override, K3 or campaign claim.
use super::*;
use kaspa_consensus_core::{
    palw_material_release_recovery_v2::PalwMaterialRecoveryRowV2,
    palw_provider_court_v1::{PALW_PROVIDER_COURT_MLDSA87_CONTEXT_V1 as DOMAIN, palw_provider_court_message_v1},
};
use misaka_palw_remote::{
    kernel_material_context_v4::KernelMaterialContextArchiveV4,
    monetary_material_gc::{
        MonetaryGcCompletedV2, MonetaryMaterialGcJanitorV1, MonetaryMaterialGcOracleV1, RemoteFinalityAuthorityV1,
        RemoteProofGcOracleV2, native_v4::native_material_release_finalized_v4,
    },
    monetary_material_rpc::{MonetaryMaterialProofPagerV1, ProvenMonetarySnapshotV2},
};

#[test]
fn rfc08_typed_terminal_release_finalized_janitor_gc_all_generations_and_cold_wal_replay() {
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", &child_name("typed_writer_child"), "--nocapture", "--test-threads=1"])
        .env("MISAKA_TYPED_WRITER_CHILD_V4", "1")
        .env("MISAKA_TYPED_REPLACEMENT_CHILD_V4", "1")
        .env("MISAKA_TYPED_TERMINAL_CHILD_V4", "1")
        .status()
        .unwrap();
    assert!(status.success(), "actual Typed terminal/finalized janitor fixture failed: {status}");
}

#[derive(borsh::BorshSerialize, borsh::BorshDeserialize)]
struct TerminalInput {
    tail: Hash64,
    release_block: Hash64,
    finalized: bool,
}

pub(super) async fn release_and_collect(
    mut w: World,
    mut db: std::sync::Arc<kaspa_database::prelude::DB>,
    dir: &std::path::Path,
    request: &kaspa_consensus_core::palw_prospective_kernel_material_v4::PalwProspectiveKernelMaterialRequestV4,
) -> (World, std::sync::Arc<kaspa_database::prelude::DB>) {
    let root = request.intent.manifest.root;
    let admission: SignedAdmission = borsh::from_slice(&std::fs::read(dir.join("signed-admission")).unwrap()).unwrap();
    let replacement: SignedReplacement = borsh::from_slice(&std::fs::read(dir.join("signed-replacement")).unwrap()).unwrap();
    let deadline = w.net.daa() + 1500;
    let tail = loop {
        let row = w.net.chain.tip_state().1.monetary_material_row_v1(root).unwrap();
        assert_eq!((row.sequence, row.consent), (1, replacement.consent));
        if let Some(tail) = row.tail_block {
            break tail;
        }
        assert!(w.net.daa() < deadline, "actual paid claim liability must yield its native tail");
        let t = w.net.ttpb();
        w.net.chain.heartbeat(t, vec![]).await;
    };
    let release = loop {
        let action = Monetary::Release { root, consent: replacement.consent, tail_block: tail };
        let signer = w.net.bond(1);
        let payload = borsh::to_vec(&action).unwrap();
        let message = palw_provider_court_message_v1(
            Hash64::from_bytes(w.net.chain.tip_state().1.kernel_route().unwrap().header.policy.network_domain),
            242,
            &signer,
            &payload,
        );
        let key = TestConsensus::palw_v2_registry_keypair(1);
        let signature =
            libcrux_ml_dsa::ml_dsa_87::sign(&key.signing_key, message.as_byte_slice(), DOMAIN, [0xB8; 32]).unwrap().as_ref().to_vec();
        let object = Obj::MonetaryMaterialActionV1 { action: Box::new(action), signer, signature };
        if matches!(
            w.net.chain.ctx.consensus.palw_object_rehearsal_v1(&object),
            Some(kaspa_consensus_core::palw_producer_v2::PalwObjectRehearsalV1::Accepted)
        ) {
            break object;
        }
        assert!(w.net.daa() < deadline, "native finality/ancestry must admit the signed Release");
        let t = w.net.ttpb();
        w.net.chain.heartbeat(t, vec![]).await;
    };
    w.net.send(vec![(1, release)]).await;
    let receipt = match w.net.chain.tip_state().1.material_recovery_row_v2(root).unwrap() {
        PalwMaterialRecoveryRowV2::Terminal(r) => r,
        _ => panic!("actual signed Release must write its terminal receipt"),
    };
    assert_eq!(receipt.final_consent, replacement.consent);
    assert_eq!(receipt.tail_block, tail);
    assert_eq!(receipt.contract.generations.len(), 2);
    for consent in [admission.consent, replacement.consent] {
        assert!(receipt.covers(root, consent));
        assert!(w.net.chain.ctx.consensus.palw_material_terminal_receipt_finalized_v2(root, consent).unwrap().is_none());
    }
    let mut phase = TerminalInput { tail, release_block: receipt.release_block, finalized: false };
    std::fs::write(dir.join("terminal-input"), borsh::to_vec(&phase).unwrap()).unwrap();
    (w, db) = reader_roundtrip(w, db, dir, request, true, true, false);
    let pin = loop {
        if let Some((pin, actual)) =
            w.net.chain.ctx.consensus.palw_material_terminal_receipt_finalized_v2(root, admission.consent).unwrap()
        {
            assert_eq!(actual, receipt);
            if pin != receipt.release_block {
                break pin;
            }
        }
        assert!(w.net.daa() < deadline, "real native finality must pass the Release block");
        let t = w.net.ttpb();
        w.net.chain.heartbeat(t, vec![]).await;
    };
    assert_ne!(pin, receipt.release_block, "snapshot pin is not the saved deletion intent's Release block");
    eprintln!("TYPED_TERMINAL_V4 finalized_pin={pin} release_block={} daa={}", receipt.release_block, w.net.daa());
    phase.finalized = true;
    std::fs::write(dir.join("terminal-input"), borsh::to_vec(&phase).unwrap()).unwrap();
    let state_root = w.net.chain.tip_state().1.state_root();
    (w, db) = reader_roundtrip(w, db, dir, request, true, true, false);
    assert_eq!(w.net.chain.tip_state().1.state_root(), state_root, "physical GC cannot mutate accepted native state");
    (w, db)
}

struct NativeOracle<'a>(&'a dyn ConsensusApi);
impl MonetaryMaterialGcOracleV1 for NativeOracle<'_> {
    fn covenant_rows(
        &mut self,
    ) -> Result<(Hash64, Vec<kaspa_consensus_core::palw_monetary_material_v1::PalwMonetaryMaterialRowV1>), String> {
        Ok((self.0.get_sink(), self.0.palw_monetary_material_covenants_v1()?))
    }
    fn chain_after(&mut self, low: Hash64, limit: usize) -> Result<Vec<Hash64>, String> {
        self.0.get_virtual_chain_from_block(low, Some(limit)).map(|p| p.added).map_err(|e| e.to_string())
    }
    fn release_finalized(&mut self, r: Hash64, c: Hash64, t: Hash64, b: Hash64) -> Result<bool, String> {
        native_material_release_finalized_v4(self.0, r, c, t, b)
    }
    fn terminal_receipt(
        &mut self,
        root: Hash64,
        consent: Hash64,
    ) -> Result<Option<(Hash64, kaspa_consensus_core::palw_material_release_recovery_v2::PalwMaterialTerminalReceiptV2)>, String> {
        self.0.palw_material_terminal_receipt_finalized_v2(root, consent)
    }
}
struct NativePin<'a> {
    native: &'a dyn ConsensusApi,
    pin: Hash64,
}
impl RemoteFinalityAuthorityV1 for NativePin<'_> {
    fn finalized_pin(&mut self) -> Result<Hash64, String> {
        Ok(self.pin)
    }
    fn selected_parent(&mut self, b: Hash64) -> Result<Hash64, String> {
        self.native.get_ghostdag_data(b).map(|g| g.selected_parent).map_err(|e| e.to_string())
    }
    fn is_chain_ancestor_of(&mut self, low: Hash64, high: Hash64) -> Result<bool, String> {
        if low == high {
            return Ok(true);
        }
        let p = self.native.get_virtual_chain_from_block(low, None).map_err(|e| e.to_string())?;
        Ok(p.removed.is_empty() && p.added.contains(&high))
    }
    fn chain_after(&mut self, low: Hash64, limit: usize) -> Result<Vec<Hash64>, String> {
        self.native.get_virtual_chain_from_block(low, Some(limit)).map(|p| p.added).map_err(|e| e.to_string())
    }
}
fn snapshot(native: &dyn ConsensusApi, pin: Hash64) -> Result<ProvenMonetarySnapshotV2, String> {
    let page = native.palw_monetary_aux_page_v1(pin, 0, 0, 1 << 20)?;
    let mut pager = MonetaryMaterialProofPagerV1::new(pin, None, &page, 4 << 20, 32 << 20, 4)?;
    pager.push(&page)?;
    while pager.cursor().0 < page.total_rows {
        let (row, offset) = pager.cursor();
        pager.push(&native.palw_monetary_aux_page_v1(pin, row, offset, 1 << 20)?)?;
    }
    pager.finish_with_recovery_v2()
}

// An explicit read-fault control AFTER the real janitor has fsynced its intent.
// It returns no fabricated positive native authority.
struct Interrupt<O> {
    oracle: O,
    refuse_witness: bool,
}
impl<O: MonetaryMaterialGcOracleV1> MonetaryMaterialGcOracleV1 for Interrupt<O> {
    fn covenant_rows(
        &mut self,
    ) -> Result<(Hash64, Vec<kaspa_consensus_core::palw_monetary_material_v1::PalwMonetaryMaterialRowV1>), String> {
        self.oracle.covenant_rows()
    }
    fn chain_after(&mut self, low: Hash64, limit: usize) -> Result<Vec<Hash64>, String> {
        self.oracle.chain_after(low, limit)
    }
    fn terminal_receipt(
        &mut self,
        r: Hash64,
        c: Hash64,
    ) -> Result<Option<(Hash64, kaspa_consensus_core::palw_material_release_recovery_v2::PalwMaterialTerminalReceiptV2)>, String> {
        self.oracle.terminal_receipt(r, c)
    }
    fn release_finalized(&mut self, r: Hash64, c: Hash64, t: Hash64, b: Hash64) -> Result<bool, String> {
        if self.refuse_witness {
            Err("explicit fixture interruption after durable deletion intent".into())
        } else {
            self.oracle.release_finalized(r, c, t, b)
        }
    }
}

pub(super) fn read_and_collect(
    native: &dyn ConsensusApi,
    input: &ReaderInput,
    dir: &std::path::Path,
    limits: PalwKernelMaterialContextLimitsV4,
) {
    let phase: TerminalInput = borsh::from_slice(&std::fs::read(dir.join("terminal-input")).unwrap()).unwrap();
    let admission: SignedAdmission = borsh::from_slice(&std::fs::read(dir.join("signed-admission")).unwrap()).unwrap();
    let replacement: SignedReplacement = borsh::from_slice(&std::fs::read(dir.join("signed-replacement")).unwrap()).unwrap();
    let root = input.request.intent.manifest.root;
    for card in [2, 3, 4, 5] {
        let consent = if card < 4 { admission.consent } else { replacement.consent };
        let file = dir.join(format!("provider-{card}"));
        let store = Provider::open_with_kernel_contexts_v4(&file, 4 << 20, 16 << 20, |_| None, limits).unwrap();
        assert_eq!(store.held_contracts(), vec![(consent, root)]);
        let authority = native.palw_material_terminal_receipt_finalized_v2(root, consent).unwrap();
        if !phase.finalized {
            assert!(authority.is_none());
            let material = std::fs::read(&file).unwrap();
            let contexts = std::fs::read(file.with_extension("kernel-context-v4")).unwrap();
            let mut store = store;
            assert!(
                !store
                    .gc_finalized(root, consent, consent, phase.tail, phase.release_block, |r, c, t, b| {
                        native_material_release_finalized_v4(native, r, c, t, b)
                    })
                    .unwrap()
            );
            assert_eq!(std::fs::read(&file).unwrap(), material);
            assert_eq!(std::fs::read(file.with_extension("kernel-context-v4")).unwrap(), contexts);
            continue;
        }
        let (pin, receipt) = authority.expect("actual finalized terminal receipt covers every saved generation");
        assert_ne!(pin, phase.release_block);
        assert_eq!(receipt.release_block, phase.release_block);
        assert_eq!(receipt.tail_block, phase.tail);
        assert_eq!(receipt.contract.generations.len(), 2);
        for c in [admission.consent, replacement.consent] {
            assert!(receipt.covers(root, c));
        }
        let mut local = NativeOracle(native);
        let mut remote = RemoteProofGcOracleV2 { authority: NativePin { native, pin }, proven_snapshot: |p| snapshot(native, p) };
        for oracle in [&mut local as &mut dyn MonetaryMaterialGcOracleV1, &mut remote as &mut dyn MonetaryMaterialGcOracleV1] {
            assert_eq!(oracle.terminal_receipt(root, consent).unwrap(), Some((pin, receipt.clone())));
            assert!(oracle.release_finalized(root, consent, phase.tail, phase.release_block).unwrap());
            assert!(
                !oracle.release_finalized(root, consent, phase.tail, pin).unwrap(),
                "snapshot pin must not substitute saved Release block"
            );
            assert!(!oracle.release_finalized(root, consent, pin, phase.release_block).unwrap(), "tail must match exactly");
        }
        if card % 2 == 0 {
            collect_one(store, &file, limits, root, consent, &phase, local);
        } else {
            collect_one(store, &file, limits, root, consent, &phase, remote);
        }
    }
}

fn collect_one<O: MonetaryMaterialGcOracleV1>(
    mut store: Provider,
    file: &std::path::Path,
    limits: PalwKernelMaterialContextLimitsV4,
    root: Hash64,
    consent: Hash64,
    phase: &TerminalInput,
    oracle: O,
) {
    let mut oracle = Interrupt { oracle, refuse_witness: true };
    let mut janitor = MonetaryMaterialGcJanitorV1::open(file).unwrap();
    let material = std::fs::read(file).unwrap();
    let contexts = std::fs::read(file.with_extension("kernel-context-v4")).unwrap();
    let interrupted = janitor.tick(&mut store, &mut oracle).unwrap();
    assert!(interrupted.deleted.is_empty());
    assert_eq!(interrupted.retained, 1);
    assert!(interrupted.pinned.iter().any(|(id, why)| *id == consent && why.contains("explicit fixture interruption")));
    assert_eq!(std::fs::read(file).unwrap(), material);
    assert_eq!(std::fs::read(file.with_extension("kernel-context-v4")).unwrap(), contexts);
    let intent_path = file.with_file_name(format!("{}.gc-intent", file.file_name().unwrap().to_str().unwrap()));
    let saved: MonetaryGcCompletedV2 = borsh::from_slice(&std::fs::read(&intent_path).unwrap()).unwrap();
    assert_eq!(
        (saved.intent.root, saved.intent.stored_consent, saved.intent.release_consent, saved.intent.tail, saved.intent.release_block),
        (root, consent, consent, phase.tail, phase.release_block),
        "actual janitor WAL must save exact Release tuple, never finalized pin"
    );
    drop(janitor);
    drop(store);
    let mut store = Provider::open_with_kernel_contexts_v4(file, 4 << 20, 16 << 20, |_| None, limits).unwrap();
    let mut janitor = MonetaryMaterialGcJanitorV1::open(file).unwrap();
    oracle.refuse_witness = false;
    let replay = janitor.tick(&mut store, &mut oracle).unwrap();
    assert_eq!(replay.deleted, vec![consent]);
    assert_eq!(replay.retained, 0);
    assert!(store.held_contracts().is_empty());
    assert!(!intent_path.exists());
    assert_eq!(janitor.completed_receipts().cloned().collect::<Vec<_>>(), vec![saved.clone()]);
    drop(store);
    drop(janitor);
    let store = Provider::open_with_kernel_contexts_v4(file, 4 << 20, 16 << 20, |_| None, limits).unwrap();
    assert!(store.held_contracts().is_empty());
    drop(store);
    let archive = KernelMaterialContextArchiveV4::open(&file.with_extension("kernel-context-v4"), limits).unwrap();
    assert!(archive.get(consent).unwrap().is_none(), "typed context metadata must be durably removed with raw material");
    drop(archive);
    let mut janitor = MonetaryMaterialGcJanitorV1::open(file).unwrap();
    assert_eq!(janitor.completed_receipts().cloned().collect::<Vec<_>>(), vec![saved]);
    janitor.acknowledge_metadata_cleanup(consent).unwrap();
    drop(janitor);
    assert!(MonetaryMaterialGcJanitorV1::open(file).unwrap().completed_receipts().next().is_none());
}
