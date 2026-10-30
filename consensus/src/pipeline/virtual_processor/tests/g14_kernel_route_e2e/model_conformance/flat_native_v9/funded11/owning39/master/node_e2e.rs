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

#[tokio::test]
async fn rfc02_native39_node_e2e_register_fund_final_gate_c_pnr9_coinbase_spend_reorg_restart() {
    use kaspa_database::{create_temp_db, prelude::ConnBuilder};
    kaspa_core::log::try_init_logger("warn");
    let began = std::time::Instant::now();
    let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let (sender, receiver) = async_channel::unbounded();
    let mut net = Net::over_cfg(mining_config_v3_budget(), |cfg| TestConsensus::with_db(db.clone(), cfg, sender));
    net._keep.push(Box::new(receiver));
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

    // ---- 4. the mining line: 73 register, 74 seat, 75 commit, Gate C draw and approvals, 77 receipt ----
    let wire = NativeMiningModelWireV39 { version: 39, template_master: c.master, sequence: 0, share_permille: 0 };
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
        borsh::to_vec(&NativeMiningCommitWireV39 { version: 39, model, master: c.master, sequence: 0 }).unwrap(),
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

    // ---- 8. restart A over its database ----
    let (sink, root, route, utxos) = (net.chain.sink(), net.chain.tip_state().1.state_root(), net.api(), Net::utxos_of(&net.chain));
    let mut net = net.restart(db.clone());
    assert_eq!(net.chain.sink(), sink, "the restarted node's sink");
    assert_eq!(net.chain.tip_state().1.state_root(), root, "the PALW state root off disk");
    assert_eq!(net.api(), route, "the kernel route rows off disk");
    assert_eq!(Net::utxos_of(&net.chain), utxos, "the UTXO set off disk");
    assert!(receipt_record(&net.chain.tip_state().1, &receipt).unwrap().is_spent(1));
    net.chain.heartbeat(ttpb, Vec::new()).await;
    // The third node: every block of A's past (side blocks of the reorged-away branch included) in DAG order, as an IBD takes them.
    let replay = t12_genesis_chain(&net.config, &net.bundle, &net.premine, &net.floats);
    for blk in past_in_order(&net.chain, net.chain.sink()) {
        arrive(&replay, blk, "the replaying node's block").await;
    }
    net.assert_same(&replay, "native39 node E2E after reorg and restart");
    eprintln!("[n39] 8. restart restored sink/root/route/UTXO; replay agrees ({:.0}s total)", began.elapsed().as_secs_f64());
    let _ = std::fs::remove_dir_all(&c.dir);
}
