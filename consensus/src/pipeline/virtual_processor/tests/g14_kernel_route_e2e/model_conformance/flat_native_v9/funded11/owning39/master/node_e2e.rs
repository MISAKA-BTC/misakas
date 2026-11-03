//! **RFC-0002 「実ノードでの一連の動作」 for owning39 (2026-10-22)**: one in-process node (`A`, on a temp database it keeps) runs the
//! whole native39 line on a private salted genesis — the harness `mining_config()` (native39, mining39, Final27, funded11, Gate C,
//! ADR-0176 budget armed ONLY in this test's params; no shipped preset or measured-record list is touched) — with the tiny owning39
//! fixture (a two-block `Cast` program, no parameters, no download):
//!
//! 1. source (41) and class (42) registration, the complete Master (65), the producer's consent (43), the requester's Fund (44, the
//!    Master's stage 0), the honest commit (45) — the only component — an outsider's independent replay that finds no fault;
//! 2. the job's independent Final (51, card 1) and the Master's Final (66);
//! 3. **job credit delivery** — the node's own anchor/aux/payout export (`palw_state_proof_v1_impl`), checked by the auditor's
//!    functions (`check_native_state_anchor_v39`, `verify_native_aux_stream_v39`, `native_job_audit_from_stream_v39`,
//!    `native_payout_status_v39`), the paying chain block found from the bracketing payout exports and its real coinbase leaf
//!    (`palw_native_coinbase_proof_v33` / `verify_native_coinbase_v33`); the auditor's `derive_native_redemption_v39` witness over
//!    those states is its own test (`..._job_credit_delivery_witness_...`) so a refusal there is reported, not hidden;
//! 4. the model line (73), a possession seat (74), the mining claim (75) past the verifier maturity, the Gate C draw seeded by attempt
//!    blocks, every drawn bond's `Reproduced` attestation (claim-level drawn approval, `'V'` subject), certification after the
//!    prosecution window and the receipt (77);
//! 5. a **PNR9** receipt block (algo 7) spending one quantum, merged by a heartbeat whose coinbase pays the receipt producer's
//!    registered payout;
//! 6. **reorg across the redemption**: a second node `B` builds, from the PNR9 block's parent, a branch that spends two quanta (more
//!    safe weight) and out-works `A`; `A` takes it: its PNR9 spend and its coinbase leg are gone from the new selected chain, and `A`
//!    and `B` fold one PALW root (the harness's finality depth equals the coinbase spend maturity, 600 DAA, so a matured spend
//!    cannot be reorged without crossing finality);
//! 7. **real UTXO spends** on the winning branch after the coinbase spend maturity: `B`'s PNR9 worker leg and the job credit, both
//!    accepted by one chain block;
//! 8. **restart**: `A` is stopped and reopened over the same database: sink, PALW state root, kernel route rows and UTXO set are
//!    restored; it carries on, and a third node replaying `A`'s chain from genesis agrees.
//!
//! Structural test values only (the parent module's TEST mining policy), never measurements. At most three consensus instances exist
//! at once (A, B, the replaying node).
use super::*;
use crate::model::services::reachability::ReachabilityService;
use crate::model::stores::ghostdag::GhostdagStoreReader;
use crate::pipeline::virtual_processor::tests::OnetimeTxSelector;
use crate::pipeline::virtual_processor::tests::t12_round_lane_e2e::{sign_spend_many, stamp_harness_time};
use kaspa_consensus_core::block::{MutableBlock, TemplateBuildMode};
use kaspa_consensus_core::coinbase::MinerData;
use kaspa_consensus_core::palw_native_audit_proof_v39::{
    NativeAuditRowReaderV39, VerifiedNativeAuxRowsV39, verify_native_aux_stream_v39,
};
use kaspa_consensus_core::palw_native_receipt_v39::*;
use kaspa_consensus_core::palw_native_redemption_v33::{
    derive_native_redemption_v39, native_payout_status_v39, verify_native_coinbase_v33,
};
use kaspa_consensus_core::palw_native_state_anchor_v39::{
    CheckedNativeStateAnchorV39, NATIVE_ANCHOR_LABEL_V39, NativeStateEnvelopeV39, check_native_state_anchor_v39,
};
use kaspa_consensus_core::palw_state_proof_v1::PalwFactProofV1;
use kaspa_consensus_core::palw_state_v2::PalwChainStateV2;
use kaspa_consensus_core::tx::ScriptPublicKey;
use std::sync::Arc;

type BlockHash = Hash64;

fn card_pubkey(card: usize) -> Vec<u8> {
    TestConsensus::palw_v2_registry_keypair(card as u64).verification_key.as_ref().to_vec()
}
fn sign(card: usize, message: &[u8], context: &[u8]) -> Vec<u8> {
    libcrux_ml_dsa::ml_dsa_87::sign(&TestConsensus::palw_v2_registry_keypair(card as u64).signing_key, message, context, [0x39u8; 32])
        .expect("ML-DSA-87 signs")
        .as_ref()
        .to_vec()
}
/// A distinct, class-valid, unspendable miner script per tag.
fn tagged_spk(tag: u64) -> ScriptPublicKey {
    let mut payload = [0u8; 64];
    payload[..8].copy_from_slice(&tag.to_le_bytes());
    payload[8..16].copy_from_slice(b"n39-e2e!");
    kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk(&payload)
}
fn paid(block: &Block, spk: &ScriptPublicKey) -> u64 {
    block.transactions[0].outputs.iter().filter(|o| o.script_public_key == *spk).map(|o| o.value).sum()
}
/// The registered payout script of card `card`'s bond (what a coinbase pays it).
fn payout_spk(net: &Net, card: usize) -> ScriptPublicKey {
    let payload = net.chain.tip_state().1.bond(&net.bond(card)).expect("the bond").payout_payload;
    kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk(&payload.as_bytes())
}

/// Insert a block that may or may not become the sink; the verdict as text.
async fn insert(chain: &T12Chain, block: MutableBlock) -> (Block, Result<(), String>) {
    let block = block.to_immutable();
    let verdict = chain.ctx.consensus.validate_and_insert_block(block.clone()).virtual_state_task.await;
    (block, verdict.map(|_| ()).map_err(|e| e.to_string()))
}

/// Every block in `upto`'s complete parent past, including red side branches,
/// with each parent emitted before its children. A selected spine plus each
/// mergeset's local ordering does not order parents across different mergesets.
fn past_in_order(chain: &T12Chain, upto: BlockHash) -> Vec<Block> {
    let genesis = chain.config.params.genesis.hash;
    let mut emitted = std::collections::HashSet::from([genesis]);
    let mut pending = vec![(upto, false)];
    let mut out = Vec::new();
    while let Some((hash, parents_visited)) = pending.pop() {
        if emitted.contains(&hash) {
            continue;
        }
        let block = chain.ctx.consensus.get_block(hash).expect("the node holds its complete past");
        if parents_visited {
            assert!(
                block.header.parents_by_level.raw().iter().flat_map(|(_, parents)| parents).all(|parent| emitted.contains(parent)),
                "every replayed block follows all its parents"
            );
            emitted.insert(hash);
            out.push(block);
        } else {
            pending.push((hash, true));
            for parent in block.header.parents_by_level.raw().iter().flat_map(|(_, parents)| parents).rev() {
                if !emitted.contains(parent) {
                    pending.push((*parent, false));
                }
            }
        }
    }
    out
}

/// The selected chain from genesis (exclusive) to the sink, oldest first.
fn spine(chain: &T12Chain) -> Vec<BlockHash> {
    let vp = chain.vp();
    let genesis = chain.config.params.genesis.hash;
    let mut out = Vec::new();
    let mut at = chain.sink();
    while at != genesis {
        out.push(at);
        at = vp.ghostdag_store.get_selected_parent(at).unwrap();
    }
    out.reverse();
    out
}

// ---- the node's own audit export, verified with the auditor's functions ----

struct MapReader(Arc<BTreeMap<(u8, Vec<u8>), Vec<u8>>>);
impl NativeAuditRowReaderV39 for MapReader {
    fn read(&self, table: u8, key: &[u8], maximum: usize) -> Result<Option<Vec<u8>>, String> {
        match self.0.get(&(table, key.to_vec())) {
            Some(v) if v.len() > maximum => Err("row over the caller's bound".into()),
            other => Ok(other.cloned()),
        }
    }
}

/// One anchored point the node exports at `child` (the state is `child`'s selected parent's post-state).
struct Point {
    child: BlockHash,
    anchor: CheckedNativeStateAnchorV39,
    rows: VerifiedNativeAuxRowsV39<MapReader>,
    payouts: PalwFactProofV1,
}

fn point(net: &Net, child: BlockHash) -> Point {
    let vp = net.chain.vp();
    let (header, fact) =
        vp.palw_state_proof_v1_impl(child, NATIVE_ANCHOR_LABEL_V39).unwrap_or_else(|e| panic!("anchor39 export at {child}: {e}"));
    assert_eq!(fact.collection.rows.len(), 1, "one anchor capsule");
    let envelope = NativeStateEnvelopeV39::from_bytes(&fact.collection.rows[0].1).expect("the exported capsule decodes");
    let parent_hash = vp.ghostdag_store.get_selected_parent(child).unwrap();
    let parent = net.chain.ctx.consensus.get_header(parent_hash).expect("the selected parent's header");
    let anchor = check_native_state_anchor_v39(&header, child, &parent, parent_hash, &net.config.params, &fact.state, &envelope)
        .unwrap_or_else(|e| panic!("the auditor checks the node's anchor at {child}: {e}"));
    // The complete aux table, page by page through the node's own pager.
    let mut all = BTreeMap::new();
    let mut cursor = "-".to_string();
    loop {
        let request = format!("{}{cursor}", kaspa_consensus_core::palw_native_audit_page_v39::AUX_PAGE_PREFIX_V39);
        let (_, page) =
            vp.palw_state_proof_v1_impl(child, request.as_bytes()).unwrap_or_else(|e| panic!("aux page39 at {child}: {e}"));
        assert_eq!(page.state.preimage, fact.state.preimage, "every page opens the same anchored state");
        for (k, v) in &page.collection.rows {
            all.insert((k[0], k[5..].to_vec()), v.clone());
        }
        let label = String::from_utf8(page.collection.label.clone()).unwrap();
        let parts: Vec<&str> =
            label.trim_start_matches(kaspa_consensus_core::palw_native_audit_page_v39::AUX_PAGE_PREFIX_V39).split('/').collect();
        assert_eq!(parts.len(), 3, "page label {label}");
        if parts[1] == "1" {
            assert_eq!(parts[0].parse::<usize>().unwrap(), all.len(), "the pager's total is the rows read");
            break;
        }
        cursor = parts[2].to_string();
    }
    let all = Arc::new(all);
    let rows = verify_native_aux_stream_v39(
        &fact.state,
        anchor.inner(),
        all.len(),
        all.iter().map(|((t, k), v)| Ok((borsh::to_vec(&(*t, k)).unwrap(), v.clone()))),
        MapReader(all.clone()),
    )
    .unwrap_or_else(|e| panic!("the complete aux stream opens the anchored root at {child}: {e}"));
    let (_, payouts) = vp
        .palw_state_proof_v1_impl(child, kaspa_consensus_core::palw_native_audit_proof_v32::NATIVE_PAYOUT_LABEL_V32)
        .unwrap_or_else(|e| panic!("payout export at {child}: {e}"));
    Point { child, anchor, rows, payouts }
}

impl Point {
    fn audit(&self, net: &Net, job: Digest) -> kaspa_consensus_core::palw_native_job_audit_v39::CheckedNativeJobAuditV39 {
        PalwChainStateV2::native_job_audit_from_stream_v39(&self.rows, &self.anchor, &net.config.params, job, None)
            .unwrap_or_else(|e| panic!("the independent job audit at {}: {e}", self.child))
    }
    fn payout_rows(&self) -> kaspa_consensus_core::palw_native_audit_proof_v32::VerifiedNativePayoutRowsV32<'_> {
        kaspa_consensus_core::palw_native_audit_proof_v32::verify_native_payout_rows_v32(&self.payouts, self.anchor.inner())
            .expect("the payout rows open the anchored state")
    }
}

/// A PNR9 receipt block on `chain`'s sink spending `quantum` of `receipt` by card `producer`'s bond (the node's own template, algo 7).
fn pnr9_block(
    chain: &T12Chain,
    domain: Hash64,
    producer: usize,
    bond: PalwBondKeyV2,
    receipt: Hash64,
    quantum: u32,
    nonce: u64,
    miner: ScriptPublicKey,
) -> MutableBlock {
    // The EVM root commits the builder's timestamp. Arm the actual template
    // clock before execution; an active lane is never re-stamped afterwards.
    if chain.config.params.is_evm_active(chain.daa_of(chain.sink())) {
        chain.vp().template_clock.store(chain.ctx.simulated_time + 1, std::sync::atomic::Ordering::Relaxed);
    }
    let mut t = chain
        .ctx
        .consensus
        .build_block_template(MinerData::new(miner, vec![]), Box::new(OnetimeTxSelector::new(Vec::new())), TemplateBuildMode::Standard)
        .expect("a template");
    stamp_harness_time(&chain.config.params, &mut t.block.header, chain.ctx.simulated_time + 1);
    t.block.header.nonce = nonce;
    t.block.header.pow_algo_id = kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_RECEIPT_V3;
    let mut block = t.block;
    let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(&block.header);
    let spend = PalwNativeReceiptSpendV39 {
        version: 39,
        network_domain: domain,
        challenge: native_receipt_spend_challenge_v39(
            domain,
            pre_pow,
            block.header.timestamp,
            block.header.nonce,
            receipt,
            quantum,
            &bond.0,
        ),
        receipt,
        quantum_index: quantum,
        producer_bond: bond.0,
        producer_pubkey: card_pubkey(producer),
    };
    let signature = sign(producer, native_receipt_spend_id_v39(&spend).as_byte_slice(), PALW_NATIVE_RECEIPT_MLDSA87_CONTEXT_V39);
    block.header.palw_commitment = PalwNativeReceiptSpendEnvelopeV39 { spend, signature }.encode();
    block.header.finalize();
    block
}

/// The processor's own receipt-block gate (`palw_v2_check_receipt_spend`, via its test seam) on `block` against the PARENT state — the
/// sink's, which the template built on — so a refusal is named instead of silently disqualifying the block.
fn rehearse_pnr9(chain: &T12Chain, block: &MutableBlock) -> Result<(), String> {
    let vp = chain.vp();
    let (sink, state) = chain.tip_state();
    let h = &block.header;
    if !h.direct_parents().contains(&sink) {
        return Err(format!("the template's parents {:?} do not include the sink {sink}", h.direct_parents()));
    }
    let params = vp.palw_state_params_v2.clone().expect("ConsensusV2");
    let point = kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
        block: h.hash,
        daa_score: h.daa_score,
        blue_score: h.blue_score,
        subsidy: vp.coinbase_manager.calc_block_subsidy(h.daa_score),
    };
    match vp.palw_v2_check_receipt_spend_for_tests(h, &state, &params, &point)? {
        Some(e) if e.spend.version == PALW_NATIVE_RECEIPT_FOLD_VERSION_V39 => Ok(()),
        other => Err(format!("the gate did not route a native39 spend: {other:?}")),
    }
}

/// After inserting `hash`: its status, and the node's sink and tips (what a heartbeat's template will build on).
fn status_line(chain: &T12Chain, hash: BlockHash) -> String {
    format!(
        "status {:?}, sink {}, virtual parents {:?}",
        chain.ctx.consensus.block_status(hash),
        chain.sink(),
        chain.ctx.consensus.get_virtual_parents()
    )
}

fn receipt_record(net_state: &PalwChainStateV2, receipt: &Hash64) -> Option<NativeMiningReceiptRecordV39> {
    native_mining_receipt_view_v39(net_state, receipt).map(|(r, _)| r)
}

/// A spend of `inputs` (each `(outpoint, entry, signing card)`) to `to`, paying FEE.
fn spend_tx(net: &Net, inputs: &[(TransactionOutpoint, UtxoEntry, usize)], to: ScriptPublicKey) -> Transaction {
    let total: u64 = inputs.iter().map(|(_, e, _)| e.amount).sum();
    assert!(total > FEE, "the spent inputs ({total} sompi) cover the harness fee {FEE}");
    let mut tx = Transaction::new(
        crate::constants::TX_VERSION,
        inputs.iter().map(|(o, _, _)| TransactionInput::new(*o, vec![], 0, 1)).collect(),
        vec![TransactionOutput::new(total - FEE, to)],
        0,
        kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE,
        0,
        vec![],
    );
    let signers: Vec<(UtxoEntry, usize)> = inputs.iter().map(|(_, e, c)| (e.clone(), *c)).collect();
    sign_spend_many(&mut tx, &signers, net.config.params.storage_mass_parameter);
    tx
}

/// The chain block of `chain` whose acceptance data accepts `txid`, with its complete accepted id list.
fn accepting_block(
    chain: &T12Chain,
    txid: kaspa_consensus_core::tx::TransactionId,
) -> Option<(BlockHash, Vec<kaspa_consensus_core::tx::TransactionId>)> {
    for hash in spine(chain).into_iter().rev().take(64) {
        let Ok(data) = chain.ctx.consensus.get_block_acceptance_data(hash) else { continue };
        let ids: Vec<_> = data.iter().flat_map(|m| m.accepted_transactions.iter().map(|a| a.transaction_id)).collect();
        if ids.contains(&txid) {
            return Some((hash, ids));
        }
    }
    None
}

/// Steps 1–3 shared by both tests: the job through its independent Final, and the chain block that drains its single credit.
struct Delivered {
    fx: Fixture,
    c: Component,
    credit: kaspa_consensus_core::palw_native_job_v39::NativeSingleCreditV39,
    final_audit: kaspa_consensus_core::palw_native_job_audit_v39::CheckedNativeJobAuditV39,
    before_audit: kaspa_consensus_core::palw_native_job_audit_v39::CheckedNativeJobAuditV39,
    pre: Point,
    post: Point,
    pay_block: BlockHash,
    pay_header: kaspa_consensus_core::header::Header,
    cb_proof: kaspa_consensus_core::palw_native_redemption_v33::NativeCoinbaseProofV33,
    /// The paying coinbase's output indices of exactly the credit's amount to its payout script.
    candidates: Vec<u32>,
}

async fn through_credit_delivery(net: &mut Net, producer: usize, began: std::time::Instant) -> Delivered {
    let params = net.config.params.clone();
    eprintln!(
        "[n39] network {} genesis {} ttpb {} coinbase spend maturity {} finality depth {}",
        params.net,
        params.genesis.hash,
        net.ttpb(),
        params.coinbase_spend_maturity(),
        params.finality_depth()
    );
    net.beat_to(5).await;

    // ---- 1. registration, Master, consent, Fund, the honest commit ----
    let fx = fixture(net).await;
    let c = master_component(net, &fx, producer, 0, 5).await;
    assert!(outsider_filing(&fx, &c, net).is_none(), "the outsider's independent replay finds no fault in the honest commit");
    eprintln!("[n39] 1. source/class/Master/consent/Fund/commit at DAA {} ({:.0}s)", c.committed, began.elapsed().as_secs_f64());

    // ---- 2. the job's independent Final (card 1) and the Master's Final ----
    // The window `finalize` waits out before the Final (51): the credit cannot exist before this DAA.
    let final_from = c.committed + c.temporal.challenge_window + c.temporal.final_tail + 4;
    finalize(net, &c).await;
    eprintln!("[n39] 2. job Final (51) and Master Final (66) by DAA {} (window end {final_from})", net.daa());

    // ---- 3. the job's single credit: the node's export, the auditor's checks, the paying block and its coinbase ----
    net.chain.heartbeat(net.ttpb(), Vec::new()).await;
    let fin = point(net, net.chain.sink());
    let final_audit = fin.audit(net, c.job);
    let credit = final_audit.credit().cloned().expect("the independent Final carries the original single credit");
    let status = native_payout_status_v39(&final_audit, &fin.payout_rows()).expect("payout status");
    eprintln!(
        "[n39] 3. credit {} sompi to {} established {}; status at the Final point: {status:?}",
        credit.amount, credit.payout, credit.established
    );
    assert!(credit.amount > 0, "a positive original credit");
    // Walk the selected chain from the Final window: the paying block P is the chain block whose pre-state (point at P) holds the
    // credit's queue row and whose post-state (point at P's chain child) does not.
    let key = borsh::to_vec(&credit.queue).unwrap();
    let has = |p: &Point| p.payout_rows().rows().iter().any(|(k, _)| *k == key);
    let mut paying = None;
    for _ in 0..16 {
        let chain = spine(&net.chain);
        let from = chain.iter().position(|h| net.chain.daa_of(*h) >= final_from).expect("the Final window end is on the chain");
        for w in chain[from..].windows(2) {
            let (pre, post) = (point(net, w[0]), point(net, w[1]));
            if has(&pre) && !has(&post) {
                paying = Some((pre, post));
                break;
            }
        }
        if paying.is_some() {
            break;
        }
        net.chain.heartbeat(net.ttpb(), Vec::new()).await;
    }
    let (pre, post) = paying.expect("the credit's queue row is drained by a chain block within 16 blocks of the Final");
    let pay_block = pre.child;
    assert_eq!(net.chain.vp().ghostdag_store.get_selected_parent(post.child).unwrap(), pay_block);
    let before_audit = pre.audit(net, c.job);
    let (pay_header, cb_proof) = net.chain.vp().palw_native_coinbase_proof_v33(pay_block).expect("the paying block's coinbase proof");
    verify_native_coinbase_v33(&cb_proof, &pay_header, pay_block, &params).expect("the auditor verifies the real coinbase leaf");
    let credit_spk = kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk(&credit.payout.as_bytes());
    let candidates: Vec<u32> = cb_proof
        .coinbase
        .outputs
        .iter()
        .enumerate()
        .filter(|(_, o)| o.value == credit.amount && o.script_public_key == credit_spk)
        .map(|(i, _)| i as u32)
        .collect();
    assert!(!candidates.is_empty(), "the paying block's coinbase has an output of the credit's amount to its payout");
    Delivered { fx, c, credit, final_audit, before_audit, pre, post, pay_block, pay_header, cb_proof, candidates }
}

/// **The auditor's delivery witness (`derive_native_redemption_v39`) over the node's own bracketing export of the paying block.** Split
/// from the main E2E so that a refusal here is reported as its own failure and does not hide the steps after it.
#[tokio::test]
async fn rfc02_native39_node_e2e_job_credit_delivery_witness_derives_on_the_node_export() {
    kaspa_core::log::try_init_logger("warn");
    let began = std::time::Instant::now();
    let mut net = Net::over_cfg(mining_config(), TestConsensus::new);
    let d = through_credit_delivery(&mut net, 0, began).await;
    let params = net.config.params.clone();
    let cb = verify_native_coinbase_v33(&d.cb_proof, &d.pay_header, d.pay_block, &params).unwrap();
    let sp = net.chain.vp().ghostdag_store.get_selected_parent(d.pay_block).unwrap();
    let sp_header = net.chain.ctx.consensus.get_header(sp).unwrap();
    let gd = net.chain.vp().ghostdag_store.get_data(d.pay_block).unwrap();
    eprintln!(
        "[n39] 3. witness: same credit {}, same original {}, before rows/audit root {}/{}, after root {}, established {}, \
         before audit daa {}, coinbase daa {}; paying block {} daa {} ts {} blue score {}; its selected parent {sp} daa {} ts {} blue score {}; \
         paying block mergeset size {}, candidates {:?}",
        d.before_audit.credit() == Some(&d.credit),
        d.final_audit.original() == d.before_audit.original(),
        d.pre.payout_rows().state_root(),
        d.before_audit.state_root(),
        d.post.payout_rows().state_root(),
        d.credit.established,
        d.before_audit.daa(),
        cb.daa(),
        d.pay_block,
        d.pay_header.daa_score,
        d.pay_header.timestamp,
        d.pay_header.blue_score,
        sp_header.daa_score,
        sp_header.timestamp,
        sp_header.blue_score,
        gd.mergeset_size(),
        d.candidates,
    );
    let mut refusals = Vec::new();
    let delivery = d.candidates.iter().find_map(|i| {
        match derive_native_redemption_v39(&d.final_audit, &d.before_audit, &d.pre.payout_rows(), &d.post.payout_rows(), &cb, *i) {
            Ok(x) => Some(x),
            Err(e) => {
                refusals.push((*i, e));
                None
            }
        }
    });
    let delivery = delivery.unwrap_or_else(|| panic!("derive_native_redemption_v39 over the node's bracketing states: {refusals:?}"));
    assert_eq!(delivery.amount(), d.credit.amount);
    assert!(
        net.chain.ctx.consensus.get_virtual_utxo_entry(delivery.outpoint()).is_some(),
        "the delivered credit is an unspent output"
    );
    eprintln!("[n39] 3. delivery: block {}, outpoint {}, {} sompi", d.pay_block, delivery.outpoint(), delivery.amount());
    // NEGATIVE: a stale witness — the state anchored one chain block earlier (two back from the paying block's post-state) — is
    // refused even though it holds the same immutable credit: the witness must be exactly the paying block's selected-parent state.
    let earlier = net.chain.vp().ghostdag_store.get_selected_parent(d.pay_block).unwrap();
    let stale = point(&net, earlier);
    if let Ok(stale_audit) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| stale.audit(&net, d.c.job))) {
        for i in &d.candidates {
            assert!(
                derive_native_redemption_v39(&d.final_audit, &stale_audit, &stale.payout_rows(), &d.post.payout_rows(), &cb, *i)
                    .is_err(),
                "a stale witness two blocks back is refused"
            );
        }
    }
    let _ = std::fs::remove_dir_all(&d.c.dir);
}

/// Read the real selected branch's stored deltas with the production RFC-0012
/// consumer. No relabelled class, synthetic work fact or injected approval.
fn native39_settlement_facts_on_blocks(
    chain: &T12Chain,
    blocks: &[BlockHash],
    receipt: Hash64,
) -> Vec<kaspa_consensus_core::palw_native_settlement_v1::MatureUsefulWorkV1> {
    use kaspa_consensus_core::palw_native_settlement_v1::{
        NativeFactRulesV1, native_delta_evidence_v1, native_facts_and_skips_with_native39_v1,
    };
    let vp = chain.vp();
    let state = chain.tip_state().1;
    let params = vp.palw_state_params_v2.as_ref().unwrap();
    let disputes = std::collections::BTreeSet::new();
    let rules = NativeFactRulesV1 {
        state: &state,
        params,
        claims_with_open_disputes: &disputes,
        canonical_work_daa: vp.palw_canonical_work_daa,
        quantum_maturity_daa: vp.palw_exec_quantum_maturity_daa,
    };
    let mut output = Vec::new();
    for block in blocks {
        let (_, delta) = vp.palw_state_v2_store.read().delta_of(*block).unwrap();
        let evidence = native_delta_evidence_v1(&delta);
        if evidence.native39_quanta.is_empty() {
            continue;
        }
        assert!(evidence.native39_quanta.iter().all(|q| q.receipt_id == receipt));
        assert_eq!(evidence.native39_missing_receipts, 0);
        let header = chain.ctx.consensus.get_header(*block).unwrap();
        let empty = std::collections::BTreeSet::new();
        let (facts, skipped) =
            native_facts_and_skips_with_native39_v1(&rules, &evidence, &empty, &empty, (header.daa_score, header.blue_score));
        assert_eq!(
            facts.len(),
            evidence.native39_quanta.len(),
            "genuine certified native quanta become priced settlement facts: {skipped:?}"
        );
        for (fact, quantum) in facts.iter().zip(&evidence.native39_quanta) {
            assert_eq!(
                (fact.work, fact.anchor, fact.operator, fact.class),
                (quantum.work, quantum.anchor, quantum.operator, quantum.class)
            );
            assert!(fact.work > 0 && fact.matured_daa >= quantum.expires, "settlement never bypasses receipt liability maturity");
        }
        let (revoked, why) = native_facts_and_skips_with_native39_v1(
            &rules,
            &evidence,
            &empty,
            &std::collections::BTreeSet::from([receipt]),
            (header.daa_score, header.blue_score),
        );
        assert!(revoked.is_empty());
        assert_eq!(why.voided as usize, evidence.native39_quanta.len());
        output.extend(facts);
    }
    output
}

/// Durable structural evidence before an expensive certification wait. These
/// are private test funds/keys; preserving the actual journal lets a failed
/// reservation be diagnosed without recreating its whole admission history.
fn capture_native39_budget_v1(net: &Net, claim: &NativeMiningClaimRecordV39, phase: &str) {
    let (_, state) = net.chain.tip_state();
    let id = native_mining_budget_id_v39(&claim.id);
    let row = state.bond_budget().and_then(|budget| budget.claim_row(&id));
    let params = net.chain.vp().palw_state_params_v2.clone().unwrap();
    let subsidy = net.chain.vp().coinbase_manager.calc_block_subsidy(net.daa());
    let carve = params.worker_carve_at(subsidy, None);
    let cap = row.zip(params.bond_budget()).map(|(row, mirror)| {
        kaspa_consensus_core::palw_bond_budget_v1::palw_bond_budget_export_cap_v1(&mirror.policy, row.liability_sompi)
    });
    let binding = kaspa_consensus_core::palw_native_budget_binding_v1::native_budget_binding_v1(&state, id)
        .unwrap()
        .expect("actual native admission installs source-bound collateral");
    assert_eq!(binding.liability_sompi, row.expect("actual committed budget").liability_sompi);
    let native_held =
        kaspa_consensus_core::palw_native_budget_binding_v1::native_budget_held_for_bond_v1(&state, &claim.producer, net.daa())
            .unwrap();
    let committed = kaspa_consensus_core::palw_state_v2::palw_bond_committed_checked_v2(
        &state,
        &claim.producer,
        net.daa(),
        None,
        params.window_court(),
    )
    .unwrap();
    assert!(native_held >= binding.liability_sompi, "the actual mining L and possession stake stay counted");
    assert!(committed >= native_held, "shared committed capital includes the actual native source holds");
    eprintln!(
        "[n39] exact collateral nativeHeld={native_held}, commonCommitted={committed}, originalPWUminimum={}",
        claim.price.reserved_exposure
    );
    eprintln!(
        "[n39] budget {phase}: id {id}, subsidy {subsidy}, workerCarve(default) {carve}, formula2 {}, exportCap {cap:?}, row {row:?}",
        params.bond_formula_v2_applies_at(net.daa())
    );
    let directory = std::env::temp_dir().join(format!("misaka-native39-budget-{}-{}", std::process::id(), claim.claim_hash()));
    std::fs::create_dir_all(&directory).unwrap();
    let carriage = kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2::from_state(&state);
    std::fs::write(directory.join(format!("{phase}.carriage.borsh")), borsh::to_vec(&carriage).unwrap()).unwrap();
    std::fs::write(directory.join(format!("{phase}.budget.borsh")), borsh::to_vec(&row.copied()).unwrap()).unwrap();
    std::fs::write(directory.join(format!("{phase}.state-params.borsh")), borsh::to_vec(&params).unwrap()).unwrap();
    std::fs::write(
        directory.join(format!("{phase}.context.txt")),
        format!(
            "sink={} daa={} ruleset={} subsidy={} carve={} formula2={} exportCap={cap:?}\nrow={row:?}\nparams={params:?}\n",
            net.chain.sink(),
            net.daa(),
            net.config.params.consensus_params_id(),
            subsidy,
            carve,
            params.bond_formula_v2_applies_at(net.daa())
        ),
    )
    .unwrap();
    eprintln!("[n39] durable budget capture {}", directory.display());
}

/// Self-consistent attacker metadata is still refused by the network rule, both
/// before a tracked tip write and at the pruning/import installation writer.
fn assert_native39_birth_context_is_enforced(net: &Net, state: &PalwChainStateV2) {
    use kaspa_consensus_core::palw_native39_frontier_v1::*;
    use kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2;
    let params = net.chain.vp().palw_state_params_v2.clone().unwrap();
    let expected = net.config.params.palw_exec_quantum_maturity_v1();
    validate_native39_frontier_context_v1(state, &params, expected).unwrap();
    let anchors = native39_frontier_anchors_v1(state).unwrap();
    let (key, original) = &anchors[0];
    let mut changed = original.clone();
    changed.quantum_maturity_daa += 1;
    changed.matured_daa = kaspa_consensus_core::palw_native_settlement_v1::native39_matured_daa_v1(
        &params,
        changed.quantum_maturity_daa,
        changed.anchor_daa,
        changed.established_daa,
        changed.expires,
    )
    .unwrap();
    let mut carriage = PalwStateCarriageV2::from_state(state);
    carriage
        .kernel_route
        .as_mut()
        .unwrap()
        .aux
        .insert((NATIVE39_FRONTIER_TABLE_V1, key.as_bytes().to_vec()), borsh::to_vec(&changed).unwrap());
    let bytes = borsh::to_vec(&carriage).unwrap();
    let changed = borsh::from_slice::<PalwStateCarriageV2>(&bytes).unwrap().into_state(&params, None).unwrap();
    assert!(
        validate_native39_frontier_context_v1(&changed, &params, expected).is_err(),
        "exact arithmetic cannot authorize a different network maturity"
    );
    let vp = net.chain.vp();
    let mut store = vp.palw_state_v2_store.write();
    let before = store.load_tip(&params).unwrap().unwrap();
    let mut tip_batch = rocksdb::WriteBatch::default();
    assert!(store.set_tip_batch_tracked_v1(&mut tip_batch, net.chain.sink(), &changed).is_err());
    let mut pruning_batch = rocksdb::WriteBatch::default();
    assert!(store.set_pruning_snapshot_batch_tracked_v1(&mut pruning_batch, net.chain.sink(), &changed).is_err());
    let after = store.load_tip(&params).unwrap().unwrap();
    assert_eq!(
        (before.0, before.1.state_root()),
        (after.0, after.1.state_root()),
        "rejected tip/import writes leave the actual disk state intact"
    );
}

/// After heavy native sources have actually been collected, the compact ledger
/// still requires the network's birth rules. Recomputing the carriage root does
/// not authorize a different maturity/pricing context.
fn assert_closed_fact_context_is_enforced(net: &Net, state: &PalwChainStateV2) {
    use kaspa_consensus_core::palw_native_closed_facts_v1::*;
    use kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2;
    validate_fact_archives_context_v1(state, &net.config.params).unwrap();
    let rows = read_native_closed_facts_v1(state).unwrap();
    let (id, row) = rows
        .iter()
        .find(|(_, r)| r.origin.family == ClosedFactFamilyV1::NativeMining39)
        .expect("actual closed native accounting survives heavy-source GC");
    let params = net.chain.vp().palw_state_params_v2.clone().unwrap();
    for mutation in 0..3 {
        let mut changed = row.clone();
        match mutation {
            0 => changed.origin.ruleset = kaspa_consensus_core::Hash::from_bytes([0xA7; 32]),
            1 => changed.origin.canonical_work_daa = Some(u64::MAX),
            _ => {
                changed.origin.quantum_maturity_daa += 1;
                changed.fact.matured_daa = changed
                    .origin
                    .material_until_daa
                    .max(changed.origin.terminal_daa.saturating_add(changed.origin.retirement_daa))
                    .max(changed.fact.anchor_daa.saturating_add(changed.origin.quantum_maturity_daa));
            }
        }
        changed.validate_v1(*id).unwrap();
        let mut carriage = PalwStateCarriageV2::from_state(state);
        carriage
            .kernel_route
            .as_mut()
            .unwrap()
            .aux
            .insert((NATIVE_CLOSED_FACTS_TABLE_V1, id.as_bytes().to_vec()), borsh::to_vec(&changed).unwrap());
        let altered = carriage.into_state(&params, None).unwrap();
        assert!(
            validate_fact_archives_context_v1(&altered, &net.config.params).is_err(),
            "root-consistent archived metadata cannot change actual network rules ({mutation})"
        );
        let vp = net.chain.vp();
        let mut store = vp.palw_state_v2_store.write();
        let before = store.load_tip(&params).unwrap().unwrap();
        let mut batch = rocksdb::WriteBatch::default();
        assert!(store.set_tip_batch_tracked_v1(&mut batch, net.chain.sink(), &altered).is_err());
        let mut batch = rocksdb::WriteBatch::default();
        assert!(store.set_pruning_snapshot_batch_tracked_v1(&mut batch, net.chain.sink(), &altered).is_err());
        let after = store.load_tip(&params).unwrap().unwrap();
        assert_eq!((before.0, before.1.state_root()), (after.0, after.1.state_root()));
    }
    use kaspa_consensus_core::palw_native39_settlement_checkpoint_v1::*;
    let archive = read_native39_settlement_checkpoint_v1(state).unwrap().expect("actual native prefix decision");
    net.chain.vp().validate_native_checkpoint_headers_v1(state, 0).unwrap();
    for mutation in 0..3 {
        let mut changed = (*archive).clone();
        match mutation {
            0 => {
                changed.depth_required += 1;
                changed.depth = changed.depth.max(changed.depth_required);
                changed.policy_id = changed.policy_v1().id();
            }
            1 => changed.parent_state_root = Hash64::from_u64_word(0xBADD),
            _ => changed.point.daa_score += 1,
        }
        changed.validate_shape_v1().unwrap();
        let mut carriage = PalwStateCarriageV2::from_state(state);
        carriage.kernel_route.as_mut().unwrap().aux.insert(
            (NATIVE39_SETTLEMENT_CHECKPOINT_TABLE_V1, NATIVE39_SETTLEMENT_CHECKPOINT_KEY_V1.to_vec()),
            borsh::to_vec(&changed).unwrap(),
        );
        let altered = carriage.into_state(&params, None).unwrap();
        assert!(
            net.chain.vp().validate_native_checkpoint_headers_v1(&altered, 0).is_err(),
            "coherent archival policy/root/header-clock mutation must fail actual context ({mutation})"
        );
    }
}

#[tokio::test]
async fn rfc02_native39_node_e2e_register_fund_final_gate_c_pnr9_coinbase_spend_reorg_restart() {
    run_native39_node_e2e(false).await;
}
#[cfg(feature = "evm")]
#[tokio::test]
async fn rfc0012_native39_fixture_clock_survives_same_db_restart() {
    use kaspa_database::{create_temp_db, prelude::ConnBuilder};
    let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let (sender, receiver) = async_channel::unbounded();
    let mut net = Net::over_cfg(native39_node_campaign_config_v1(true, false), |cfg| TestConsensus::with_db(db.clone(), cfg, sender));
    net._keep.push(Box::new(receiver));
    net.chain.ctx.consensus.set_validation_time_for_test(net.chain.ctx.simulated_time);
    // Larger than the real clock lead cap: this is simulated elapsed time,
    // not a change to a header, the EVM commitment, or the network cap.
    let before = net.chain.ctx.simulated_time;
    let first = net.chain.heartbeat(600_000, Vec::new()).await;
    assert!(first.header.timestamp >= before + 600_000);
    let sink = net.chain.sink();
    let root = net.chain.tip_state().1.state_root();
    let mut net = net.restart(db.clone());
    assert_eq!(net.chain.sink(), sink);
    assert_eq!(net.chain.tip_state().1.state_root(), root);
    assert!(net.chain.ctx.consensus.has_validation_time_for_test(), "restart must retain the explicit instance-local clock mode");
    let before = net.chain.ctx.simulated_time;
    let second = net.chain.heartbeat(600_000, Vec::new()).await;
    assert!(second.header.timestamp >= before + 600_000);
    assert_eq!(net.chain.sink(), second.header.hash);
}
#[cfg(feature = "evm")]
#[tokio::test]
async fn rfc0012_native39_actual_evm_settlement_maturity_reorg_restart() {
    run_native39_node_e2e(true).await;
}
/// A distinct pre-genesis policy exercises two authenticated quantum anchors.
/// It does not claim operator/class diversification or legacy-family coverage.
#[cfg(feature = "evm")]
#[tokio::test]
async fn rfc0012_native39_actual_evm_two_quantum_policy_reorg_restart_pruned_import() {
    let mut parts = native39_node_campaign_config_v1(true, false);
    let quantum = parts.0.params.palw_native_mining_v39.as_ref().unwrap().policy.network_quantum_mac;
    let policy = &mut parts.0.params.palw_dns_retirement.as_mut().unwrap().settlement;
    policy.settled_anchor_depth = 2;
    policy.unique_mature_work = u128::from(quantum).checked_mul(2).unwrap();
    assert!(policy.valid());
    run_native39_node_e2e_with_parts_v2(parts, true, false).await;
}
#[tokio::test]
async fn rfc23s2_native39_independent_pending_claim_actual_node_reorg_restart() {
    run_native39_node_e2e_with_stage2(false, true).await;
}
#[cfg(feature = "evm")]
#[tokio::test]
async fn rfc23s2_native39_actual_evm_independent_claim_pruned_import() {
    run_native39_node_e2e_with_stage2(true, true).await;
}
mod family_joint_campaign;
mod family_legacy_campaign;
#[path = "stage2_pruned.rs"]
mod stage2_pruned;
/// One private, pre-genesis configuration for native39, actual funded text
/// claims and claim-local EVM campaigns. Callers must derive their network
/// domain from the returned genesis before constructing signed jobs/roots.
pub(crate) fn native39_node_campaign_config_v1(
    evm_settlement: bool,
    claim_local_stage2: bool,
) -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    let mut parts = mining_config_v3_budget();
    let formal_native_settlement = evm_settlement || claim_local_stage2;
    if formal_native_settlement {
        use kaspa_consensus_core::config::params::ForkActivation;
        // These are the existing explicit test policies, not shipped fences.
        // A formal native fact requires its real once-only issuance obligation,
        // the shared bond budget and the actual X3 liability deadline.
        parts.0.params.palw_consensus_mode = kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(parts.1.clone());
        parts.0.params.palw_settlement_once_v1 = Some(ForkActivation::new(0));
        parts.0.params.palw_claim_hard_deadline_v1 = Some(
            kaspa_consensus_core::palw_ruling_fences_v1::PalwClaimHardDeadlineFenceV1::unapproved_probe_at(ForkActivation::new(0)),
        );
        // The inherited mechanics harness can leave the existing V3 budget at
        // a later fence. Formal receipt issuance needs that same real policy
        // from birth, including its formula/rights accounting and state mirror.
        parts.0.params.palw_bond_budget_v1.as_mut().expect("V3 owning39 budget policy exists").activation = ForkActivation::new(0);
        parts.0.params.sync_palw_bond_budget_v1();
        assert!(parts.0.params.palw_probabilistic_constraints_v1.is_some_and(|f| f.is_active(0)));
        assert!(parts.0.params.palw_bond_budget_v1.as_ref().is_some_and(|f| f.activation.is_active(0)));
        parts.0.params.sync_palw_2026_10_12_ruling_fences_v1();
        parts.0.params.sync_palw_settlement_once_v1();
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &parts.0.params.palw_consensus_mode else {
            unreachable!()
        };
        parts.1 = bundle.clone();
    }
    #[cfg(feature = "evm")]
    if evm_settlement {
        let launched = kaspa_consensus_core::config::params::palw_t12_launch_params_v1();
        parts.0.params.evm_activation_daa_score = launched.evm_activation_daa_score;
        parts.0.params.palw_model_evm = launched.palw_model_evm;
        parts.0.params.palw_dns_retirement =
            Some(crate::pipeline::virtual_processor::tests::rfc12_zero_dns_matrix::test_retirement(0));
    }
    if claim_local_stage2 {
        // Explicit experimental node campaign configuration, not production
        // parameter validation or permission to arm the unmeasured public fork.
        parts.0.params.palw_consensus_mode = kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(parts.1.clone());
        parts.0.params.palw_claim_local_settlement_v1 = Some(kaspa_consensus_core::config::params::ForkActivation::new(0));
        parts.0.params.sync_palw_claim_local_settlement_v1();
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &parts.0.params.palw_consensus_mode else {
            panic!("stage2 campaign requires V2")
        };
        parts.1 = bundle.clone();
    }
    if formal_native_settlement {
        assert!(parts.1.state.bond_budget_active_at(0), "formal genesis uses the actual mirrored native budget");
        assert!(parts.1.state.settlement_once_active_at(0), "formal genesis uses the actual once-only issuance mirror");
    }
    parts
}

async fn run_native39_node_e2e(evm_settlement: bool) {
    run_native39_node_e2e_with_stage2(evm_settlement, false).await;
}
async fn run_native39_node_e2e_with_stage2(evm_settlement: bool, claim_local_stage2: bool) {
    run_native39_node_e2e_with_parts_v2(
        native39_node_campaign_config_v1(evm_settlement, claim_local_stage2),
        evm_settlement,
        claim_local_stage2,
    )
    .await;
}
/// Reuse the actual native receipt/UTXO/reorg/restart/pruned-import campaign on
/// one caller-selected, completely configured genesis candidate.
pub(crate) async fn run_native39_node_e2e_with_parts_v2(
    parts: (Config, PalwConsensusParamsV2, Premine, Premine),
    evm_settlement: bool,
    claim_local_stage2: bool,
) {
    use kaspa_database::{create_temp_db, prelude::ConnBuilder};
    kaspa_core::log::try_init_logger("warn");
    let began = std::time::Instant::now();
    let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let (sender, receiver) = async_channel::unbounded();
    let formal_native_settlement = evm_settlement || claim_local_stage2;
    if formal_native_settlement {
        assert!(parts.1.state.bond_budget_active_at(0));
        assert!(parts.1.state.settlement_once_active_at(0));
    }
    let mut net = Net::over_cfg(parts, |cfg| TestConsensus::with_db(db.clone(), cfg, sender));
    net._keep.push(Box::new(receiver));
    run_native39_node_e2e_on_net_v3(net, db, evm_settlement, claim_local_stage2, began).await;
}

/// Preserve one real DB-backed candidate while the joint caller installs its
/// independently signed legacy-family history before native B begins.
pub(super) async fn run_native39_node_e2e_on_net_v3(
    mut net: Net,
    db: Arc<kaspa_database::prelude::DB>,
    evm_settlement: bool,
    claim_local_stage2: bool,
    began: std::time::Instant,
) -> Net {
    let formal_native_settlement = evm_settlement || claim_local_stage2;
    if formal_native_settlement {
        // Long deterministic settlement/pruning advances use the instance-local
        // test clock, just like the EVM t12 harness. Production caps stay intact.
        net.chain.ctx.consensus.set_validation_time_for_test(net.chain.ctx.simulated_time);
    }
    let params = net.config.params.clone();
    let (producer, outsider) = (0usize, 1usize);
    let d = through_credit_delivery(&mut net, producer, began).await;
    let (c, pay_block) = (d.c, d.pay_block);
    // The credit output, located on the node's real coinbase (the auditor witness of it is the split test above).
    assert_eq!(d.candidates.len(), 1, "one output of the credit's amount to its payout in the paying coinbase");
    let credit_outpoint = TransactionOutpoint::new(d.cb_proof.coinbase.id(), d.candidates[0]);
    let credit_entry =
        net.chain.ctx.consensus.get_virtual_utxo_entry(credit_outpoint).expect("the paid credit is an unspent coinbase output");
    assert_eq!(credit_entry.amount, d.credit.amount);
    let _ = &d.fx;
    eprintln!("[n39] 3. credit paid by {pay_block}: outpoint {credit_outpoint}, {} sompi", credit_entry.amount);

    // Independent A is an actually admitted native mining claim whose drawn
    // verifiers do not approve it. No receipt, reward or supplied Final bit.
    let pending_native = if claim_local_stage2 {
        let a = master_component(&mut net, &d.fx, producer, 1, 5).await;
        finalize(&mut net, &a).await;
        let before = ids(&net, MINING_MODEL_KIND_V39);
        act(
            &mut net,
            1,
            REGISTER_MODEL_ACTION_V39,
            borsh::to_vec(&NativeMiningModelWireV39 { version: 39, template_master: a.master, sequence: 0, share_permille: 0 })
                .unwrap(),
        )
        .await;
        let model_a = new_id(&net, MINING_MODEL_KIND_V39, &before);
        act(
            &mut net,
            producer,
            MODEL_SEAT_ACTION_V39,
            borsh::to_vec(&NativeMiningSeatWireV39 { version: 39, model: model_a, kind: 0 }).unwrap(),
        )
        .await;
        let cert = net.chain.vp().palw_state_params_v2.clone().unwrap().rfc0018_v1().unwrap().certification;
        net.beat_to(cert.verifier_maturity_daa + 2).await;
        let before = ids(&net, MINING_CLAIM_KIND_V39);
        act(
            &mut net,
            producer,
            MINING_COMMIT_ACTION_V39,
            borsh::to_vec(&NativeMiningCommitWireV39 { version: 39, model: model_a, master: a.master, sequence: 0 }).unwrap(),
        )
        .await;
        let id = new_id(&net, MINING_CLAIM_KIND_V39, &before);
        let claim_a = mining_claim(&net, &id);
        assert!(claim_a.closed.is_none());
        Some((a, claim_a))
    } else {
        None
    };

    // ---- 4. the mining line: 73 register, 74 seat, 75 commit, Gate C draw and approvals, 77 receipt ----
    let wire = NativeMiningModelWireV39 {
        version: 39,
        template_master: c.master,
        sequence: u64::from(claim_local_stage2),
        share_permille: 0,
    };
    let before = ids(&net, MINING_MODEL_KIND_V39);
    act(&mut net, 1, REGISTER_MODEL_ACTION_V39, borsh::to_vec(&wire).unwrap()).await;
    let model = new_id(&net, MINING_MODEL_KIND_V39, &before);
    act(&mut net, producer, MODEL_SEAT_ACTION_V39, borsh::to_vec(&NativeMiningSeatWireV39 { version: 39, model, kind: 0 }).unwrap())
        .await;
    assert!(row(&net, MINING_SEAT_KIND_V39, &(model, net.bond(producer))).is_some(), "the producer holds a possession seat");
    let cert = net.chain.vp().palw_state_params_v2.clone().unwrap().rfc0018_v1().unwrap().certification;
    net.beat_to(cert.verifier_maturity_daa + 2).await;
    let before = ids(&net, MINING_CLAIM_KIND_V39);
    act(
        &mut net,
        producer,
        MINING_COMMIT_ACTION_V39,
        borsh::to_vec(&NativeMiningCommitWireV39 { version: 39, model, master: c.master, sequence: u64::from(claim_local_stage2) })
            .unwrap(),
    )
    .await;
    let claim = mining_claim(&net, &new_id(&net, MINING_CLAIM_KIND_V39, &before));
    eprintln!(
        "[n39] 4. model {}, mining claim {} at DAA {} price {:?}",
        Hash64::from_bytes(model),
        Hash64::from_bytes(claim.id),
        net.daa(),
        claim.price
    );
    capture_native39_budget_v1(&net, &claim, "accepted");
    let source20_birth = if claim_local_stage2 {
        let state = net.chain.tip_state().1;
        let id = Hash64::from_bytes(claim.id);
        let receipt = state.native_mining_source_summary_v20(id).unwrap().expect("actual action75 captures native scoped birth/hold");
        assert_eq!(receipt.claim, id);
        assert_eq!(receipt.birth.daa_score, claim.admitted_daa);
        assert_eq!(receipt.original_root, Hash64::from_bytes(claim.root()));
        assert_eq!(receipt.latest_root, receipt.original_root);
        assert_eq!(receipt.anchor_references, 0);
        assert!(receipt.collected.is_none());
        // Compare with the REAL original admission delta, not current policy or
        // a synthetic native row. The hold's exact jobs are written by action75.
        let (_, delta) = net.chain.vp().palw_state_v2_store.read().delta_of(receipt.birth.block).unwrap();
        let actual_h = delta
            .entries
            .iter()
            .filter_map(|e| match e {
                kaspa_consensus_core::palw_state_v2::PalwDeltaEntryV2::KernelRouteRow { table, key, new: Some(raw), .. }
                    if *table == MINING_TABLE_V39 && key.len() == 66 && key[..2] == [39, MINING_JOB_HOLD_KIND_V39] =>
                {
                    let (owner, until): ([u8; 64], u64) = borsh::from_slice(raw).unwrap();
                    (owner == claim.id).then_some(until)
                }
                _ => None,
            })
            .max()
            .expect("actual mining birth has its actual source holds");
        assert_eq!(receipt.liability_until, actual_h);
        let cold = kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2::from_state(&state)
            .into_state(&net.bundle.state, Some(state.state_root()))
            .unwrap();
        assert_eq!(cold.native_mining_source_summary_v20(id).unwrap(), Some(receipt));
        // Genuine admitted state, with ZERO old frames and ZERO native anchors.
        // This isolates the new scoped source's actual construction ownership.
        let mut scoped = kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2::from_state(&state);
        scoped.kernel_route.as_mut().unwrap().aux.retain(|(table, key), _| {
            !(*table == kaspa_consensus_core::palw_claim_local_v1::PALW_CLAIM_LOCAL_TABLE_V1 && key.len() == 65 && key[0] == b'F')
        });
        let scoped = scoped.into_state(&net.bundle.state, None).expect("actual current scoped sources reconstruct with zero F1");
        assert!(kaspa_consensus_core::palw_native39_frontier_v1::native39_frontier_anchors_v1(&scoped).unwrap().is_empty());
        let point = *scoped.last_point().unwrap();
        let before = scoped.state_root();
        let requests = kaspa_consensus_core::palw_state_v2::palw_claim_local_checkpoint_construction_requests_v24(
            &scoped,
            &net.bundle.state,
            &point,
        )
        .unwrap();
        assert!(requests > 0);
        use kaspa_consensus_core::palw_native_resource_v1::{NativeDemandCheckerV1, NativeSnapshotResourceLimitsV1};
        let exact = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: requests, database_bytes: 0 });
        let owned = kaspa_consensus_core::palw_state_v2::palw_claim_local_owned_checkpoint_with_checker_v20(
            &scoped,
            &net.bundle.state,
            &point,
            &exact,
        )
        .unwrap();
        assert_eq!(exact.outstanding(), (requests, 0));
        assert!(owned.checkpoint().is_some());
        let facts = owned.allocation_footprint_v20().unwrap().unwrap();
        assert!(
            facts.heap_bytes <= requests && facts.canonical_bytes <= requests && facts.material_bytes <= requests,
            "actual typed output fits the declared inventory: {facts:?} / {requests}"
        );
        let logical = kaspa_consensus_core::palw_state_v2::palw_claim_local_checkpoint_snapshot_v2(&scoped).unwrap().unwrap();
        let work = kaspa_consensus_core::palw_claim_local_v1::PalwClaimLocalWorkRefV1::NativeMiningClaim39 { claim: id };
        assert_eq!(
            logical.records[&work].material.len(),
            4,
            "actual receipt header/body/current native row/explicit certification are retained; repeated current tuple dedups"
        );
        assert!(facts.records >= 1 && facts.material_rows >= 4 && facts.material_references >= 4);
        drop(owned);
        assert_eq!(exact.outstanding(), (0, 0), "actual owned output and local capacity release together");
        let short = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: requests - 1, database_bytes: 0 });
        assert!(matches!(
            kaspa_consensus_core::palw_state_v2::palw_claim_local_owned_checkpoint_with_checker_v20(
                &scoped,
                &net.bundle.state,
                &point,
                &short
            ),
            Err(kaspa_consensus_core::palw_state_v2::PalwStateV2Error::NativeResource(_))
        ));
        assert_eq!(short.outstanding(), (0, 0));
        assert_eq!(scoped.state_root(), before, "one-under refuses before pure checkpoint construction changes any source/state");
        Some(receipt)
    } else {
        None
    };
    let ttpb = net.ttpb();
    for round in 0..64 {
        if cert_row(&net, &claim).is_some_and(|r| r.anchor.is_some()) {
            break;
        }
        net.chain.attempt(5 + (round % 3), ttpb, Vec::new(), &|_| true).await;
        let to = net.daa() + 8;
        net.beat_to(to).await;
    }
    let drawn = cert_row(&net, &claim).expect("the certification row");
    assert!(
        drawn.anchor.is_some() && drawn.draws.len() == cert.m as usize,
        "Gate C drew m = {} positions: {:?}",
        cert.m,
        drawn.outcome
    );
    let attestors = attest_all(&mut net, &claim, &[]).await;
    let row_now = cert_row(&net, &claim).unwrap();
    assert!(row_now.positive_positions() >= cert.k as u32, "k = {} of m positive: {}", cert.k, row_now.positive_positions());
    net.beat_to(drawn.final_daa + cert.prosecution_window_daa + 2).await;
    let r = cert_row(&net, &claim).unwrap();
    assert!(r.outcome.is_some_and(|(o, _)| o.certifies()), "SecurityCertified after the prosecution window: {:?}", r.outcome);
    act(
        &mut net,
        outsider,
        ISSUE_RECEIPT_ACTION_V39,
        borsh::to_vec(&NativeMiningSubjectWireV39 { version: 39, kind: 0, id: claim.id }).unwrap(),
    )
    .await;
    let receipt = Hash64::from_bytes(native_mining_receipt_id_v39(claim.id));
    let rc = receipt_record(&net.chain.tip_state().1, &receipt).expect("the receipt (77) is live");
    eprintln!(
        "[n39] 4. Gate C: {} attestors, certified; receipt {receipt}: quanta {}, expires {}, DAA now {} ({:.0}s)",
        attestors.len(),
        rc.price.quanta,
        rc.expires,
        net.daa(),
        began.elapsed().as_secs_f64()
    );
    assert!(rc.price.quanta >= 2, "the reorg branch spends two quanta: the fixture receipt has {}", rc.price.quanta);
    capture_native39_budget_v1(&net, &claim, "receipt-before-gate");

    if let Some((_, a)) = &pending_native {
        use kaspa_consensus_core::palw_claim_local_v1::PalwClaimLocalWorkRefV1 as W;
        let state = net.chain.tip_state().1;
        assert!(
            mining_claim(&net, &a.id).closed.is_none(),
            "A remains an unissued native claim while independent B gets an actual certified receipt"
        );
        let _validated = kaspa_consensus_core::palw_state_v2::palw_claim_local_owned_checkpoint_v1(
            &state,
            &net.bundle.state,
            state.last_point().unwrap(),
        )
        .unwrap()
        .expect("the actual current checkpoint validates before reading its records");
        let checkpoint = kaspa_consensus_core::palw_state_v2::palw_claim_local_checkpoint_snapshot_v2(&state)
            .unwrap()
            .expect("the current checkpoint has a logical snapshot");
        let a_record = &checkpoint.records[&W::NativeMiningClaim39 { claim: Hash64::from_bytes(a.id) }];
        assert!(!a_record.final_claim && !a_record.certified && !a_record.liability_closed && !a_record.material.is_empty());
        let b_record = &checkpoint.records[&W::NativeMiningClaim39 { claim: Hash64::from_bytes(claim.id) }];
        assert!(b_record.final_claim && b_record.certified, "B's actual Gate C approval is retained independently");
        assert!(a_record.accepted_blue_score < b_record.accepted_blue_score);
        let full_frames = state
            .kernel_route_state_v1()
            .unwrap()
            .aux
            .iter()
            .filter(|((table, key), _)| {
                *table == kaspa_consensus_core::palw_claim_local_v1::PALW_CLAIM_LOCAL_TABLE_V1 && key.len() == 65 && key[0] == b'F'
            })
            .map(|(_, raw)| kaspa_consensus_core::palw_claim_local_v1::palw_liability_material_id_v1(raw))
            .collect::<std::collections::BTreeSet<_>>();
        assert!(
            a_record.material.is_disjoint(&full_frames) && b_record.material.is_disjoint(&full_frames),
            "native mining checkpoints retain scoped actual birth/hold material, with no F1 carriage"
        );
        let mut missing = kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2::from_state(&state);
        let route = missing.kernel_route.as_mut().unwrap();
        let body = route
            .aux
            .keys()
            .find(|(table, key)| {
                *table == kaspa_consensus_core::palw_claim_local_v1::PALW_CLAIM_LOCAL_TABLE_V1
                    && key.len() == 65
                    && key[0] == b'B'
                    && b_record.material.contains(&Hash64::from_bytes(key[1..].try_into().unwrap()))
            })
            .cloned()
            .expect("the real native receipt owns a scoped literal body");
        route.aux.remove(&body);
        assert!(
            missing.into_state(&net.bundle.state, None).is_err(),
            "present scoped owner with missing real material never downgrades to still-retained old F1"
        );
        let id = Hash64::from_bytes(claim.id);
        let native = state.native_mining_source_summary_v20(id).unwrap().unwrap();
        assert_ne!(native.birth.block, native.updated.block, "the real receipt update follows its original admission");
        let mut partial_history = kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2::from_state(&state);
        let route = partial_history.kernel_route.as_mut().unwrap();
        let born_key = kaspa_consensus_core::palw_claim_local_v1::palw_claim_liability_frame_key_v1(native.birth.block);
        let update_key = kaspa_consensus_core::palw_claim_local_v1::palw_claim_liability_frame_key_v1(native.updated.block);
        assert!(route.aux.contains_key(&(kaspa_consensus_core::palw_claim_local_v1::PALW_CLAIM_LOCAL_TABLE_V1, born_key.to_vec())));
        assert!(
            route.aux.remove(&(kaspa_consensus_core::palw_claim_local_v1::PALW_CLAIM_LOCAL_TABLE_V1, update_key.to_vec())).is_some()
        );
        let partial = partial_history
            .into_state(&net.bundle.state, None)
            .expect("newer actual scoped native current supersedes a retained earlier birth F1");
        assert_eq!(partial.native_mining_source_summary_v20(id).unwrap(), Some(native));
        let partial_checkpoint =
            kaspa_consensus_core::palw_state_v2::palw_claim_local_checkpoint_snapshot_v2(&partial).unwrap().unwrap();
        assert_eq!(
            partial_checkpoint.records[&W::NativeMiningClaim39 { claim: id }],
            *b_record,
            "removing only the later F1 preserves exact native birth/H and current credit decision"
        );
    }

    // ---- 5. the PNR9 receipt block on A, merged by a heartbeat whose coinbase pays the producer's registered payout ----
    let (bond, leg_spk) = (net.bond(producer), payout_spk(&net, producer));
    net.chain.heartbeat(ttpb, Vec::new()).await;
    let fork = net.chain.sink();
    let sim_at_fork = net.chain.ctx.simulated_time;
    let (_, at_fork) = net.chain.tip_state();
    // PNR9 authenticates the producer's chosen header payout; unlike PFS4 it
    // carries no separate builder-fee authorization or automatic payout split.
    let r1 = pnr9_block(&net.chain, net.domain, producer, bond, receipt, 0, 0x3901, leg_spk.clone());
    rehearse_pnr9(&net.chain, &r1).unwrap_or_else(|e| {
        let budget_id = kaspa_consensus_core::palw_native_receipt_v39::native_mining_budget_id_v39(&claim.id);
        panic!(
            "the node's receipt-block gate refuses A's PNR9 block: {e}; reservation {:?}",
            at_fork.bond_budget().and_then(|budget| budget.claim_row(&budget_id))
        )
    });
    let (r1, v) = insert(&net.chain, r1).await;
    v.unwrap_or_else(|e| panic!("the PNR9 receipt block is accepted: {e}"));
    eprintln!("[n39] 5. PNR9 {} inserted: {}", r1.header.hash, status_line(&net.chain, r1.header.hash));
    let m = net.chain.heartbeat_paying(MinerData::new(tagged_spk(0x4D), vec![]), ttpb, Vec::new()).await;
    let m_gd = net.chain.vp().ghostdag_store.get_data(m.header.hash).unwrap();
    assert!(
        m_gd.selected_parent == r1.header.hash || m_gd.unordered_mergeset_without_selected_parent().any(|b| b == r1.header.hash),
        "the heartbeat's coinbase pays the receipt block: heartbeat parents {:?}, receipt block {}",
        m.header.direct_parents(),
        status_line(&net.chain, r1.header.hash)
    );
    let (_, after_m) = net.chain.tip_state();
    assert!(receipt_record(&after_m, &receipt).unwrap().is_spent(0), "the chain folds the PNR9 spend of quantum 0");
    assert!(
        after_m.safe_weight() > at_fork.safe_weight(),
        "the PNR9 spend credits safe weight ({} -> {})",
        at_fork.safe_weight(),
        after_m.safe_weight()
    );
    let a_settlement = if formal_native_settlement {
        native39_settlement_facts_on_blocks(&net.chain, &[r1.header.hash, m.header.hash], receipt)
    } else {
        Vec::new()
    };
    if formal_native_settlement {
        assert_eq!(a_settlement.len(), 1, "one actual issuance obligation creates exactly one RFC-0012 quantum");
        let anchors = kaspa_consensus_core::palw_native39_frontier_v1::native39_frontier_anchors_v1(&after_m).unwrap();
        assert_eq!(anchors.len(), 1);
        assert_eq!(
            (anchors[0].1.anchor, anchors[0].1.matured_daa),
            (a_settlement[0].anchor, a_settlement[0].matured_daa),
            "frontier and settlement use one actual accepted quantum and one liability clock"
        );
    }
    if evm_settlement {
        assert_native39_birth_context_is_enforced(&net, &after_m);
        let evaluated = net.chain.vp().native_evaluate(net.chain.sink()).snapshot;
        assert!(a_settlement[0].matured_daa > net.daa());
        assert_eq!(evaluated.safe, None, "an authentic receipt remains immature through its complete native liability horizon");
        assert_eq!(evaluated.finalized, None);
    }
    let leg = paid(&m, &leg_spk);
    let leg_index = m.transactions[0].outputs.iter().position(|o| o.script_public_key == leg_spk);
    eprintln!("[n39] 5. PNR9 {} (algo 7) merged by {}: producer payout leg {leg} sompi", r1.header.hash, m.header.hash);
    assert!(leg > 0, "the merging coinbase pays the receipt producer's registered payout");
    let leg_outpoint = TransactionOutpoint::new(m.transactions[0].id(), leg_index.unwrap() as u32);

    assert!(net.chain.ctx.consensus.get_virtual_utxo_entry(leg_outpoint).is_some(), "the PNR9 leg is a coinbase output on A");

    // ---- 6. reorg across the redemption: node B, from `fork`, spends quanta 0 and 1 and out-works A ----
    // testnet-12's harness has finality depth = coinbase spend maturity = 600 DAA, so the reorg crosses the PNR9 spend and its coinbase
    // delivery (not a matured spend: that would be a reorg below finality); the matured UTXO spend is made on the winning branch (7).
    let a_work = net.chain.ctx.consensus.get_header(net.chain.sink()).unwrap().blue_work;
    let mut b = t12_genesis_chain(&net.config, &net.bundle, &net.premine, &net.floats);
    if formal_native_settlement {
        b.ctx.consensus.set_validation_time_for_test(sim_at_fork);
    }
    for blk in past_in_order(&net.chain, fork) {
        arrive(&b, blk, "B's replay up to the fork").await;
    }
    assert_eq!(b.sink(), fork);
    b.ctx.simulated_time = sim_at_fork;
    let mut branch = Vec::new();
    let mut b_mergers = Vec::new();
    for (q, nonce) in [(0u32, 0xB0u64), (1, 0xB1)] {
        let r = pnr9_block(&b, net.domain, producer, bond, receipt, q, nonce, leg_spk.clone());
        rehearse_pnr9(&b, &r).unwrap_or_else(|e| panic!("the node's receipt-block gate refuses B's PNR9 block for quantum {q}: {e}"));
        let (r, v) = insert(&b, r).await;
        v.unwrap_or_else(|e| panic!("B's PNR9 block for quantum {q} is accepted: {e}"));
        branch.push(r);
        let hb = b.heartbeat(ttpb, Vec::new()).await;
        b_mergers.push(hb.clone());
        branch.push(hb);
    }
    while b.ctx.consensus.get_header(b.sink()).unwrap().blue_work <= a_work {
        branch.push(b.heartbeat(ttpb, Vec::new()).await);
    }
    let b_spent = receipt_record(&b.tip_state().1, &receipt).unwrap();
    assert!(b_spent.is_spent(0) && b_spent.is_spent(1), "B folds both PNR9 spends");
    for blk in &branch {
        arrive(&net.chain, blk.clone(), "B's branch block").await;
    }
    assert_eq!(net.chain.sink(), b.sink(), "A reorgs onto B's branch ({} blocks)", branch.len());
    assert_eq!(net.chain.tip_state().1.state_root(), b.tip_state().1.state_root(), "A and B fold one PALW root after the reorg");
    let on_a: Vec<BlockHash> = spine(&net.chain);
    for (what, h) in [("A's PNR9 block", r1.header.hash), ("A's merging heartbeat", m.header.hash)] {
        assert!(!on_a.contains(&h), "{what} is off the selected chain after the reorg");
    }
    assert!(net.chain.ctx.consensus.get_virtual_utxo_entry(leg_outpoint).is_none(), "A's PNR9 coinbase leg is gone after the reorg");
    let selected_native_blocks: Vec<_> = branch.iter().map(|block| block.header.hash).collect();
    let b_settlement = if formal_native_settlement {
        native39_settlement_facts_on_blocks(&net.chain, &selected_native_blocks, receipt)
    } else {
        Vec::new()
    };
    if formal_native_settlement {
        assert_eq!(b_settlement.len(), 2, "reorg replacement contains two real quanta, not A plus B");
        assert_ne!(b_settlement[0].identity, b_settlement[1].identity, "quantum identity counts work exactly once");
        let (_, replacement) = net.chain.tip_state();
        let anchors = kaspa_consensus_core::palw_native39_frontier_v1::native39_frontier_anchors_v1(&replacement).unwrap();
        assert_eq!(anchors.len(), 2);
        if claim_local_stage2 {
            let summary = replacement.native_mining_source_summary_v20(Hash64::from_bytes(claim.id)).unwrap().unwrap();
            assert_eq!(summary.anchor_references, 2, "native receipt references follow the actual reorg replacement only");
            let born = source20_birth.unwrap();
            assert_eq!(
                (summary.birth, summary.original_root, summary.liability_until),
                (born.birth, born.original_root, born.liability_until),
                "reorg does not rebase the original native consent or source hold"
            );
            let cold = kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2::from_state(&replacement)
                .into_state(&net.bundle.state, Some(replacement.state_root()))
                .unwrap();
            assert_eq!(cold.native_mining_source_summary_v20(Hash64::from_bytes(claim.id)).unwrap(), Some(summary));
        }
        assert!(
            anchors.iter().all(|(_, anchor)| anchor.anchor != m.header.hash),
            "reorg removes abandoned native anchors along with their work and coinbase leg"
        );
    }
    let rc_after = receipt_record(&net.chain.tip_state().1, &receipt).unwrap();
    assert!(
        rc_after.is_spent(0) && rc_after.is_spent(1) && rc_after.spent_quanta == 2,
        "the new tip: quanta 0 and 1, each spent once"
    );
    let merger = b_mergers[0].clone();
    let b_leg = paid(&merger, &leg_spk);
    assert!(b_leg > 0, "B's merging coinbase pays the producer's leg for B's PNR9 spend of quantum 0");
    let b_leg_outpoint = TransactionOutpoint::new(
        merger.transactions[0].id(),
        merger.transactions[0].outputs.iter().position(|o| o.script_public_key == leg_spk).unwrap() as u32,
    );
    eprintln!("[n39] 6. reorg: A took B's branch ({} blocks); A's leg {leg} gone; B's leg {b_leg} at {b_leg_outpoint}", branch.len());
    drop(b);

    // ---- 7. the real UTXO spends after the coinbase spend maturity, on the winning branch ----
    net.beat_to(merger.header.daa_score + params.coinbase_spend_maturity() + 2).await;
    let leg_entry =
        net.chain.ctx.consensus.get_virtual_utxo_entry(b_leg_outpoint).expect("B's PNR9 leg is an unspent coinbase output on A");
    let spend_leg = spend_tx(&net, &[(b_leg_outpoint, leg_entry.clone(), producer)], tagged_spk(0x5E1));
    net.mempool(&spend_leg).unwrap_or_else(|e| panic!("the mempool takes the PNR9 leg spend: {e}"));
    let funding = net.funding[producer].clone();
    let spend_credit = spend_tx(
        &net,
        &[(credit_outpoint, credit_entry.clone(), producer), (funding.0, funding.1.clone(), producer)],
        tagged_spk(0x5E2),
    );
    net.mempool(&spend_credit).unwrap_or_else(|e| panic!("the mempool takes the credit spend: {e}"));
    net.funding[producer] =
        (TransactionOutpoint::new(spend_credit.id(), 0), UtxoEntry::new(spend_credit.outputs[0].value, tagged_spk(0x5E2), 0, false));
    let carrying = net.chain.heartbeat(ttpb, vec![spend_leg.clone(), spend_credit.clone()]).await;
    assert!(
        carrying.transactions.iter().any(|t| t.id() == spend_leg.id())
            && carrying.transactions.iter().any(|t| t.id() == spend_credit.id())
    );
    net.chain.heartbeat(ttpb, Vec::new()).await;
    for (what, o) in [("PNR9 leg", b_leg_outpoint), ("job credit", credit_outpoint)] {
        assert!(net.chain.ctx.consensus.get_virtual_utxo_entry(o).is_none(), "the {what} output is spent");
    }
    assert!(net.chain.ctx.consensus.get_virtual_utxo_entry(TransactionOutpoint::new(spend_leg.id(), 0)).is_some());
    let (accepting, accepted) = accepting_block(&net.chain, spend_credit.id()).expect("a chain block accepts the credit spend");
    assert!(accepted.contains(&spend_leg.id()), "the same chain block accepts the PNR9 leg spend");
    eprintln!("[n39] 7. spends: leg {} and credit {} accepted by chain block {accepting}", spend_leg.id(), spend_credit.id());

    if evm_settlement {
        let maturity = b_settlement.iter().map(|fact| fact.matured_daa).max().unwrap();
        net.beat_to((maturity + 2).max(params.pruning_depth().saturating_add(2))).await;
        let settlement_policy = params.palw_dns_retirement.unwrap().settlement;
        if settlement_policy.settled_anchor_depth == 2 {
            use kaspa_consensus_core::palw_native_settlement_v1::certify_native_effect_v1;
            assert_eq!(b_settlement.len(), 2);
            assert_ne!(b_settlement[0].anchor, b_settlement[1].anchor);
            let first = b_settlement.iter().min_by_key(|f| (f.accepted_blue, f.accepted_daa)).unwrap();
            let order = (first.accepted_daa, first.accepted_blue);
            let pair_work: u128 = b_settlement.iter().map(|f| f.work).sum();
            assert_eq!(
                pair_work, settlement_policy.unique_mature_work,
                "W is exactly two actual native-priced quanta, without clipping or invented work"
            );
            for fact in &b_settlement {
                assert!(fact.work < settlement_policy.unique_mature_work);
                assert!(
                    certify_native_effect_v1(settlement_policy, order, net.daa(), true, true, true, true, std::slice::from_ref(fact))
                        .is_err(),
                    "each authenticated fact is insufficient on its own"
                );
            }
            certify_native_effect_v1(settlement_policy, order, net.daa(), true, true, true, true, &b_settlement)
                .expect("the real pair satisfies the same depth/work arithmetic consumed by production evaluation");
        }
        for _ in 0..64 {
            if net.chain.ctx.consensus.get_native_settlement_snapshot().unwrap().is_some_and(|s| s.finalized.is_some()) {
                break;
            }
            net.chain.heartbeat(ttpb, Vec::new()).await;
        }
        let sink = net.chain.sink();
        let evaluated = net.chain.vp().native_evaluate(sink).snapshot;
        let published = net
            .chain
            .ctx
            .consensus
            .get_native_settlement_snapshot()
            .unwrap()
            .expect("native settlement published by actual EVM blocks");
        assert_eq!(evaluated, published, "the published certificate is exactly the production native evaluation");
        assert!(published.safe.is_some(), "mature independently certified PNR9 work advances actual EVM Safe: {published:?}");
        assert!(published.unique_work.parse::<u128>().unwrap() > 0);
        let heads = net.chain.ctx.consensus.get_evm_canonical_heads().unwrap().unwrap();
        assert_eq!(
            (heads.latest_head(), heads.safe_head(), heads.finalized_head()),
            (published.latest, published.safe, published.finalized)
        );
        // Finalized is constrained by the node's real validated pruning point.
        // Genesis has no EVM execution; waiting through actual pruning is needed
        // for a finalized label, never a test-side pruning-point replacement.
        assert!(
            published.finalized.is_some(),
            "actual pruning must retain and finalize a prefix under mature native work: {published:?}"
        );
        // Release the actual receipt/claim through action79, then let the normal
        // finite journal GC retire its heavy source capsules. No state mutation
        // or fabricated fact substitutes for the native admission history.
        let (_, before_collect) = net.chain.tip_state();
        let retained = kaspa_consensus_core::palw_native39_frontier_v1::native39_frontier_anchors_v1(&before_collect).unwrap();
        let release_at = retained
            .iter()
            .map(|(_, a)| kaspa_consensus_core::palw_native39_frontier_v1::native39_frontier_retained_until_v1(&net.bundle.state, a))
            .max()
            .unwrap();
        net.beat_to(release_at.saturating_add(2)).await;
        act(
            &mut net,
            outsider,
            COLLECT_MINING_ACTION_V39,
            borsh::to_vec(&NativeMiningSubjectWireV39 { version: 39, kind: 1, id: receipt.as_bytes() }).unwrap(),
        )
        .await;
        for _ in 0..3 {
            net.chain.heartbeat(ttpb, Vec::new()).await;
        }
        let (_, collected) = net.chain.tip_state();
        assert!(receipt_record(&collected, &receipt).is_none(), "actual collection releases the source receipt");
        if claim_local_stage2 {
            assert!(
                collected.native_mining_source_summary_v20(Hash64::from_bytes(claim.id)).unwrap().is_none(),
                "actual collection plus final anchor retirement releases the scoped birth/hold owner"
            );
            let cold = kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2::from_state(&collected)
                .into_state(&net.bundle.state, Some(collected.state_root()))
                .unwrap();
            assert!(cold.native_mining_source_summary_v20(Hash64::from_bytes(claim.id)).unwrap().is_none());
        }
        assert!(
            kaspa_consensus_core::palw_native39_frontier_v1::native39_frontier_anchors_v1(&collected).unwrap().is_empty(),
            "finite retention releases all heavy native capsules"
        );
        assert!(
            !kaspa_consensus_core::palw_native_closed_facts_v1::read_native_closed_facts_v1(&collected).unwrap().is_empty(),
            "compact closed identities preserve actual work after source GC"
        );
        assert!(
            kaspa_consensus_core::palw_native39_settlement_checkpoint_v1::read_native39_settlement_checkpoint_v1(&collected)
                .unwrap()
                .is_some(),
            "actual policy certificate survives source GC"
        );
        let after_gc = net.chain.ctx.consensus.get_native_settlement_snapshot().unwrap().unwrap();
        assert_eq!(net.chain.vp().native_evaluate(net.chain.sink()).snapshot, after_gc);
        let archive = kaspa_consensus_core::palw_native39_settlement_checkpoint_v1::read_native39_settlement_checkpoint_v1(&collected)
            .unwrap()
            .unwrap();
        let pruning = net.chain.ctx.consensus.pruning_point();
        if net.chain.vp().reachability_service.try_is_chain_ancestor_of(pruning, archive.effect.block).unwrap() {
            assert!(
                after_gc.safe.is_some() && after_gc.finalized.is_some(),
                "a covered executed archival prefix keeps positive Safe/Finalized after heavy-source collection"
            );
        } else if after_gc.finalized.is_none() {
            // RFC0012 recomputes public labels. A certificate below the current
            // pruning walk cannot be relabelled as execution of a newer block.
            let readiness = net.chain.vp().native_safe_readiness(net.chain.sink()).unwrap();
            assert!(
                readiness.finalized.withdrawn_from.is_some(),
                "a real label withdrawal is explicit; equal None is not preserved positive finality"
            );
            let history = net.chain.vp().native_finalized_publication().unwrap().unwrap();
            assert!(history.finalized.is_some(), "incident history retains the actual earlier publication");
        }
        eprintln!(
            "[n39] post-GC archive effect {}, actual PP {}, public safe {:?}, finalized {:?}",
            archive.effect.block, pruning, after_gc.safe, after_gc.finalized
        );
        if !claim_local_stage2 {
            assert_closed_fact_context_is_enforced(&net, &collected);
        }
    }

    // ---- 8. restart A over its database ----
    let (sink, root, route, utxos) = (net.chain.sink(), net.chain.tip_state().1.state_root(), net.api(), Net::utxos_of(&net.chain));
    let mut net = net.restart(db.clone());
    assert_eq!(net.chain.sink(), sink, "the restarted node's sink");
    assert_eq!(net.chain.tip_state().1.state_root(), root, "the PALW state root off disk");
    assert_eq!(net.api(), route, "the kernel route rows off disk");
    assert_eq!(Net::utxos_of(&net.chain), utxos, "the UTXO set off disk");
    if evm_settlement {
        assert!(receipt_record(&net.chain.tip_state().1, &receipt).is_none());
    } else {
        assert!(receipt_record(&net.chain.tip_state().1, &receipt).unwrap().is_spent(1));
    }
    net.chain.heartbeat(ttpb, Vec::new()).await;
    if evm_settlement {
        let snapshot = net.chain.ctx.consensus.get_native_settlement_snapshot().unwrap().unwrap();
        let state = net.chain.tip_state().1;
        let archive = kaspa_consensus_core::palw_native39_settlement_checkpoint_v1::read_native39_settlement_checkpoint_v1(&state)
            .unwrap()
            .unwrap();
        if net
            .chain
            .vp()
            .reachability_service
            .try_is_chain_ancestor_of(net.chain.ctx.consensus.pruning_point(), archive.effect.block)
            .unwrap()
        {
            assert!(
                snapshot.safe.is_some() && snapshot.finalized.is_some(),
                "restart preserves positive public heads for a covered archived executed prefix"
            );
        }
        stage2_pruned::exercise(&net, pending_native.as_ref().map(|(_, claim)| Hash64::from_bytes(claim.id))).await;
    }
    // The legacy campaign retains its independent full-history replay. The
    // EVM campaign has just exercised a real pruned join above; it must not
    // require bodies which ordinary source pruning is allowed to discard.
    if !evm_settlement {
        let replay = t12_genesis_chain(&net.config, &net.bundle, &net.premine, &net.floats);
        for blk in past_in_order(&net.chain, net.chain.sink()) {
            arrive(&replay, blk, "the replaying node's block").await;
        }
        net.assert_same(&replay, "native39 node E2E after reorg and restart");
    }
    eprintln!(
        "[n39] 8. restart restored sink/root/route/UTXO; independent import agrees ({:.0}s total)",
        began.elapsed().as_secs_f64()
    );
    if let Some((a, claim_a)) = &pending_native {
        assert!(
            mining_claim(&net, &claim_a.id).closed.is_none(),
            "independent A is still explicitly retained after B's real payment/reorg/restart"
        );
        let _ = std::fs::remove_dir_all(&a.dir);
    }
    let _ = std::fs::remove_dir_all(&c.dir);
    net
}

pub(crate) mod actual_fp_wire_source_v1;

#[cfg(test)]
mod original_archive_node_flow_v1;
