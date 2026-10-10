//! **RFC-0009 mandatory adversarial test 4 at chain-block level: the miner goes offline after its free-prompt claim, and ANOTHER bonded
//! builder's block redeems the claim's winning quantum into the miner's registered payout** — `palw_receipt_spend_v4`, fence 4 of
//! testnet-12's int-13 list (`PALW_T12_INT13_FENCES_V1`), on the real consensus pipeline (`t12-daa9000-drill-plan.md` §1, §3.4).
//!
//! The ruleset is `t12_int13_flag_day_crossing`'s: testnet-12's whole release compressed to heights a test reaches, with the int-13 list
//! (all four fences) at [`H`]. Every block below is the node's own template, inserted through `validate_and_insert_block`; every PALW
//! object rides a funded transaction; nothing is planted in state. What the chain is shown to do:
//!
//! 1. **Below the fence** a `PFS4` receipt header is refused at the header stage exactly as the live testnet-12 build refuses it — its shape
//!    gate has no `PFS4` arm, so the carriage is over the 8,192-byte cap (A-2 uniformity; the by-name `BelowFence` is the second lock).
//! 2. **The claim**: the floor's free-prompt lane is certified on chain (`FamilyCertified`, `ClassLaneCertified`); the executor runs a
//!    caller's prompt on the floor (`Base0Backend`), publishes the capture to two independent DA providers (a directory and the
//!    127.0.0.1 reference HTTP server — read back verified), commits the claim on a 0x4a carrier funded from its own float, signs ONE
//!    position-free `RDA4` authorization, files it with the same providers — and goes offline: its key, node and process are not used
//!    again by the scenario (two marked probes use the key as an ADVERSARY or a counterfactual, never to help the redemption: the V3
//!    twin of step 5, a separate node computing what V3 would have paid, and the V3 re-spend of step 6).
//! 3. **Public verification from providers only**: the chain binds the panel at the anchor slot; each seat fetches the material from the
//!    providers alone (`fetch_claim_material_any`, judged against the claim's on-chain roots), checks it with the floor's own
//!    `verify_material` against the chain's roots, output root and job pin — and only then signs `Valid`. The quorum is assembled by the
//!    node's own assembler, carried on a funded 0x4b carrier, and the claim reaches `Final` on the chain.
//! 4. **Another builder redeems**: past the claim's draw slot, builder A (another genesis bond, another key) mirrors the authorizations
//!    from the providers (`sync_redemptions_v1`) and runs kaspad's builder mode step for step (`produce_redemption`: the node's template
//!    with the builder's miner data, `palw_fp_spendable_v3` for the win, algo 7 + `PFS4`). The block is accepted and takes the tip (a
//!    receipt block is a chain block like any other: its own fold applies the spend); the next heartbeat's coinbase pays it.
//! 5. **The coinbase** of that heartbeat pays builder A's script exactly the authorization's 500 bps of the receipt block's worker reward
//!    and the executor bond's registered payout the miner leg — and, against a V3 counterfactual twin node where the executor stayed online
//!    and spent the same quantum itself at the same position, every other output, the total, the safe weight and the PALW claim row are
//!    the same: the only difference is the fee carve.
//! 6. **Single use**: a sibling `PFS4` block (builder B, same parent) arrives one block later — the two siblings are the same PALW state,
//!    so the fork choice keeps A's branch or takes B's on the hash, and either way the selected chain pays ONE fee and ONE leg; a V3 spend
//!    of the same quantum by the executor's own bond after that is `StatusDisqualifiedFromChain` and paid nothing when merged.
//! 7. **Reorg**: a spend-less branch from P that is LONGER in blue work is refused (the canonical chain is the PALW candidate order, not
//!    the header blue work); a branch from P that the PALW order prefers — two OTHER winning quanta redeemed by builder C — takes the
//!    node, and A's quantum is unspent on the new tip; on that branch two builders race for it in ONE mergeset — exactly one is paid, the
//!    executor's leg once — and a node that replays the winning branch from genesis (across `H`) reaches the same sink and PALW root.
//! 8. A `PFS4` carriage above the 8,192-byte cap rides only past the fence; a `PFS3` one above the cap is still refused past it.
//!
//! Runtime: ~10–15 min (one ~700-block chain and three replays of it: the twin, the reorg branch and the replaying node).
//!
//! **Not covered here (stated, not implied):** the provider court (miner-to-provider DA responsibility transfer, RFC-0009 Part B) is not
//! folded, so the material's availability after the miner leaves is the providers' goodwill, not an obligation; kaspad's builder mode
//! itself is not linked (this crate cannot depend on kaspad) — its decision procedure is reproduced line for line in
//! [`builder_mode_pick`]; and nothing ran on a multi-node network (that is the drill's leg, `audit-combined/rfc9-v4-leg.sh`).
use super::OnetimeTxSelector;
use super::t12_int13_flag_day_crossing::{H, t12_release};
use super::t12_round_lane_e2e::{T12Chain, card_payout_spk, sign_spend, stamp_harness_time, t12_genesis_chain};
use crate::consensus::test_consensus::TestConsensus;
use crate::model::stores::ghostdag::GhostdagStoreReader;
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::block::{Block, MutableBlock, TemplateBuildMode};
use kaspa_consensus_core::coinbase::MinerData;
use kaspa_consensus_core::config::Config;
use kaspa_consensus_core::palw_backend::{PalwClaimRootsV1, PalwExecutionBackendV1, PalwMaterialVerdictV1};
use kaspa_consensus_core::palw_freeprompt_v3::{PalwFpSpendableQuantumV3, fp_job_id_v3};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusParamsV2;
use kaspa_consensus_core::palw_receipt_v4::{
    PALW_COMMITMENT_MAX_BYTES_V4, PALW_RECEIPT_V4_AUTH_MLDSA87_CONTEXT, PALW_RECEIPT_V4_BEACON_RULE_SLOT,
    PALW_RECEIPT_V4_BUILDER_FEE_CAP_BPS, PALW_RECEIPT_V4_SPEND_MLDSA87_CONTEXT, PALW_RECEIPT_V4_VERSION, PalwReceiptSpendEnvelopeV4,
    PalwReceiptSpendUnsignedV4, PalwRedemptionAuthBundleV4, PalwRedemptionAuthV4, fp_spend_id_v4, palw_receipt_v4_miner_script,
    palw_receipt_v4_split_v1, redeem_auth_id_v4, spend_challenge_v4,
};
use kaspa_consensus_core::palw_state_v2::{
    PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwClaimSourceV2, PalwConsensusObjectV2 as Obj,
};
use kaspa_consensus_core::pow_layer0::{PALW_COMMITMENT_MAX_BYTES, POW_ALGO_ID_PALW_RECEIPT_V3};
use kaspa_consensus_core::tx::{ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry};
use kaspa_hashes::Hash64;
use libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair;
use misaka_palw_base0::backend::Base0Backend;
use misaka_palw_remote::transport::{
    EvidenceProvider, ProviderSpecV1, fetch_claim_material_any, open_providers_v1, publish_redemption_v1, publish_to_providers_v1,
    server, sync_redemptions_v1,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The executor (the miner who goes offline). Never a seat on its own claim; never a builder.
const EXECUTOR: usize = 3;
/// Pays the two certification carriers (anyone may: neither object is signed).
const CERTIFIER: usize = 1;
/// The builders: other genesis bonds, other keys.
const BUILDER_A: usize = 5;
const BUILDER_B: usize = 6;
const BUILDER_C: usize = 7;
/// The authorization's builder fee (the chain's cap is 1,000 bps).
const FEE_BPS: u16 = 500;
/// A carrier's fee, generous (100 MSK floats).
const CARRIER_FEE: u64 = 50_000_000;

type Premine = Vec<(TransactionOutpoint, UtxoEntry)>;

fn card_key(i: usize) -> &'static MLDSA87KeyPair {
    TestConsensus::palw_v2_registry_keypair(i as u64)
}

fn card_pubkey(i: usize) -> Vec<u8> {
    card_key(i).verification_key.as_ref().to_vec()
}

fn sign(i: usize, message: &[u8], context: &[u8]) -> Vec<u8> {
    libcrux_ml_dsa::ml_dsa_87::sign(&card_key(i).signing_key, message, context, [0x49u8; 32])
        .expect("ML-DSA-87 signs")
        .as_ref()
        .to_vec()
}

fn h64(n: u64) -> Hash64 {
    Hash64::from_u64_word(n)
}

/// A distinct, class-valid, unspendable miner script per tag (the ML-DSA-87 P2PKH class over a fixed payload).
fn tagged_spk(tag: u64) -> ScriptPublicKey {
    let mut payload = [0u8; 64];
    payload[..8].copy_from_slice(&tag.to_le_bytes());
    payload[8..16].copy_from_slice(b"rfc9-v4e");
    kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk(&payload)
}

/// The coinbase's outputs, summed per script.
fn by_script(block: &Block) -> BTreeMap<Vec<u8>, u64> {
    let mut m = BTreeMap::new();
    for o in &block.transactions[0].outputs {
        *m.entry(o.script_public_key.script().to_vec()).or_default() += o.value;
    }
    m
}

fn paid(block: &Block, spk: &ScriptPublicKey) -> u64 {
    by_script(block).get(spk.script()).copied().unwrap_or(0)
}

/// Each card's spendable float (its 100 MSK genesis fee float, then each carrier's change).
struct Wallets(Vec<(TransactionOutpoint, UtxoEntry)>);

impl Wallets {
    /// A carrier on `subnetwork` paid from `card`'s float: one input, the change back to the card. The change is the card's next float.
    fn carrier(
        &mut self,
        card: usize,
        subnetwork: kaspa_consensus_core::subnets::SubnetworkId,
        payload: Vec<u8>,
        config: &Config,
    ) -> Transaction {
        self.carrier_paying(card, subnetwork, payload, CARRIER_FEE, config)
    }

    /// [`Self::carrier`] paying `fee` (a certification chunk pays its slot rent on top of its carriage).
    fn carrier_paying(
        &mut self,
        card: usize,
        subnetwork: kaspa_consensus_core::subnets::SubnetworkId,
        payload: Vec<u8>,
        fee: u64,
        config: &Config,
    ) -> Transaction {
        let (outpoint, entry) = self.0[card].clone();
        let change = entry.amount - fee;
        let mut tx = Transaction::new(
            crate::constants::TX_VERSION,
            vec![TransactionInput::new(outpoint, vec![], 0, 1)],
            vec![TransactionOutput::new(change, card_payout_spk(card))],
            0,
            subnetwork,
            0,
            payload,
        );
        sign_spend(&mut tx, entry, card, config.params.storage_mass_parameter);
        self.0[card] = (TransactionOutpoint::new(tx.id(), 0), UtxoEntry::new(change, card_payout_spk(card), 0, false));
        tx
    }
}

fn lifecycle_payload(object: Obj) -> Vec<u8> {
    use kaspa_consensus_core::palw_lifecycle_objects_v2::{PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2};
    borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object }).expect("serializes")
}

fn network_domain(config: &Config) -> Hash64 {
    kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
        config.params.net.to_string().as_bytes(),
        Some(config.params.genesis.hash),
    )
}

/// **The node's own template, as a receipt block** — exactly what kaspad's producer does (`get_block_template` with this bond's miner data,
/// then `pow_algo_id = 7`), stamped with the harness clock (the EVM lane is inert here).
fn receipt_template(chain: &T12Chain, miner: ScriptPublicKey, timestamp: u64, nonce: u64) -> MutableBlock {
    let mut t = chain
        .ctx
        .consensus
        .build_block_template(MinerData::new(miner, vec![]), Box::new(OnetimeTxSelector::new(Vec::new())), TemplateBuildMode::Standard)
        .expect("a template");
    stamp_harness_time(&chain.config.params, &mut t.block.header, timestamp);
    t.block.header.nonce = nonce;
    t.block.header.pow_algo_id = POW_ALGO_ID_PALW_RECEIPT_V3;
    t.block
}

/// **The builder's V4 spend** over the block's position (`ValidatorKey::build_fp_receipt_spend_envelope_v4`, field for field): the
/// executor's authorization and signature whole, the builder's bond, key and signature.
fn sign_v4(
    block: &mut MutableBlock,
    domain: Hash64,
    builder: usize,
    builder_bond: TransactionOutpoint,
    bundle: &PalwRedemptionAuthBundleV4,
    quantum: u32,
    beacon_block: Hash64,
) {
    let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(&block.header);
    let a = &bundle.authorization;
    let spend = PalwReceiptSpendUnsignedV4 {
        version: PALW_RECEIPT_V4_VERSION,
        network_domain: domain,
        challenge: spend_challenge_v4(
            domain,
            pre_pow,
            block.header.timestamp,
            block.header.nonce,
            a.claim_id,
            quantum,
            &a.executor_bond,
            &builder_bond,
        ),
        claim_id: a.claim_id,
        quantum_index: quantum,
        beacon_block,
        executor_bond: a.executor_bond,
        builder_bond,
        builder_pubkey: card_pubkey(builder),
        authorization: a.clone(),
        executor_pubkey: bundle.executor_pubkey.clone(),
        authorization_signature: bundle.signature.clone(),
    };
    let builder_signature = sign(builder, fp_spend_id_v4(&spend).as_byte_slice(), PALW_RECEIPT_V4_SPEND_MLDSA87_CONTEXT);
    block.header.palw_commitment = PalwReceiptSpendEnvelopeV4 { spend, builder_signature }.encode();
    block.header.finalize();
}

/// **A V3 spend** (`PFS3`) of `(claim, quantum)` by `producer`'s bond over the block's position — the pre-RFC-0009 receipt, which requires
/// the producer to BE the claim's executor.
fn sign_v3(
    block: &mut MutableBlock,
    domain: Hash64,
    producer: usize,
    producer_bond: TransactionOutpoint,
    claim: Hash64,
    quantum: u32,
    beacon_block: Hash64,
) {
    use kaspa_consensus_core::palw_freeprompt_v3::{
        PALW_FP_V3_MLDSA87_SPEND_CONTEXT, PALW_FP_V3_VERSION, PalwReceiptSpendEnvelopeV3, PalwReceiptSpendUnsignedV3, fp_spend_id_v3,
        spend_challenge_v3,
    };
    let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(&block.header);
    let spend = PalwReceiptSpendUnsignedV3 {
        version: PALW_FP_V3_VERSION,
        network_domain: domain,
        challenge: spend_challenge_v3(domain, pre_pow, block.header.timestamp, block.header.nonce, claim, quantum, &producer_bond),
        claim_id: claim,
        quantum_index: quantum,
        beacon_block,
        producer_bond,
        producer_pubkey: card_pubkey(producer),
    };
    let signature = sign(producer, fp_spend_id_v3(&spend).as_byte_slice(), PALW_FP_V3_MLDSA87_SPEND_CONTEXT);
    block.header.palw_commitment = PalwReceiptSpendEnvelopeV3 { spend, signature }.encode();
    block.header.finalize();
}

/// **kaspad's builder mode** (`PalwProducerService::produce_redemption`), decision for decision, over the node's own `ConsensusApi`: the
/// fence at the next DAA; the directory's bundles in name order; a bundle that fails its own checks is skipped, never believed; an expired
/// one is skipped; the claim's spendable quanta (`palw_fp_spendable_v3` of the executor bond) must hold a WINNING, unspent quantum inside
/// the authorized range and its use window.
fn builder_mode_pick(chain: &T12Chain, dir: &Path, domain: Hash64) -> Option<(PalwRedemptionAuthBundleV4, PalwFpSpendableQuantumV3)> {
    let next_daa = chain.ctx.consensus.get_virtual_daa_score();
    if !chain.config.params.palw_receipt_spend_v4_active_at(next_daa) {
        return None;
    }
    let mut names: Vec<PathBuf> = std::fs::read_dir(dir).ok()?.filter_map(|e| e.ok().map(|e| e.path())).collect();
    names.sort();
    for path in names {
        let Ok(bytes) = std::fs::read(&path) else { continue };
        let Ok(bundle) = PalwRedemptionAuthBundleV4::decode(&bytes) else { continue };
        if bundle
            .validate_v4(domain, |key, message, sig, context| {
                kaspa_txscript::verify_mldsa87_with_context(key, message, sig, context).unwrap_or(false)
            })
            .is_err()
        {
            continue;
        }
        let auth = &bundle.authorization;
        if next_daa > auth.expiry_daa {
            continue;
        }
        let spendable = chain.ctx.consensus.palw_fp_spendable_v3(auth.executor_bond);
        let Some(win) = spendable.into_iter().find(|q| {
            q.claim_id == auth.claim_id
                && q.wins
                && next_daa <= q.spend_deadline_daa
                && q.quantum_index >= auth.quantum_lo
                && q.quantum_index < auth.quantum_hi
        }) else {
            continue;
        };
        return Some((bundle, win));
    }
    None
}

/// Insert a block that is not expected to become the sink (a receipt block); the verdict as text.
async fn insert(chain: &T12Chain, block: MutableBlock) -> (Block, Result<(), String>) {
    let block = block.to_immutable();
    let verdict = chain.ctx.consensus.validate_and_insert_block(block.clone()).virtual_state_task.await;
    (block, verdict.map(|_| ()).map_err(|e| e.to_string()))
}

/// Feed `blocks` to `to`, each through the real consensus path, in order.
async fn feed(to: &T12Chain, blocks: &[Block], what: &str) {
    for block in blocks {
        to.ctx
            .consensus
            .validate_and_insert_block(block.clone())
            .virtual_state_task
            .await
            .unwrap_or_else(|e| panic!("{what}: block {} (DAA {}) was refused: {e}", block.header.hash, block.header.daa_score));
    }
}

fn fp_spent(state: &PalwChainStateV2, claim: &Hash64) -> Vec<u32> {
    match &state.claim(claim).expect("the claim stands").source {
        PalwClaimSourceV2::FreePrompt { spent, .. } => spent.iter().copied().collect(),
        other => panic!("a free-prompt claim, not {other:?}"),
    }
}

/// The scratch directory every provider of this run keeps its files in.
fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rfc9-v4-chain-e2e-{tag}-{}-{}", std::process::id(), rand::random::<u32>()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

/// What the executor leaves behind when it goes offline: public facts only (no key, no node, no process).
struct LeftBehind {
    claim_id: Hash64,
    executor_bond: TransactionOutpoint,
}

#[tokio::test]
async fn rfc9_v4_chain_e2e_the_offline_miner_is_paid_by_another_builders_block() {
    kaspa_core::log::try_init_logger("warn");
    let (config, bundle, premine, floats): (Config, PalwConsensusParamsV2, Premine, Premine) = t12_release(true);
    assert!(config.params.palw_receipt_spend_v4_active_at(H) && !config.params.palw_receipt_spend_v4_active_at(H - 1));
    let ttpb = config.params.target_time_per_block();
    let domain = network_domain(&config);
    let floor = bundle.base_class_id;
    let maturity = bundle.freeprompt.receipt_maturity_daa();
    let use_window = bundle.freeprompt.receipt_use_window_daa();
    let mut chain = t12_genesis_chain(&config, &bundle, &premine, &floats);
    let bonds = chain.bonds.clone();
    let executor_spk = palw_receipt_v4_miner_script(&chain.tip_state().1.bond(&bonds[EXECUTOR]).unwrap().payout_payload);
    assert_eq!(executor_spk, card_payout_spk(EXECUTOR), "the executor bond's registered payout is its card's harness payout");
    let mut wallets = Wallets(floats.clone());
    eprintln!(
        "[rfc9-v4] int-13 at {H}; receipt maturity {maturity}, use window {use_window}; anchor delay {}",
        bundle.panel.anchor_delay()
    );

    // ---- the independent DA providers: a directory and the reference HTTP server on 127.0.0.1 ----------------------------------------
    let root = scratch("providers");
    let http =
        server::start("127.0.0.1:0", root.join("http"), server::ServerConfig::default()).expect("the reference provider starts");
    let specs = vec![ProviderSpecV1::Http(http.url()), ProviderSpecV1::Dir(root.join("dir"))];
    let providers = open_providers_v1(&specs);
    let provs: Vec<&dyn EvidenceProvider> = providers.iter().map(|p| p.as_ref()).collect();

    // ---- 1. below the fence: a PFS4 receipt header is refused at the header stage, by name -------------------------------------------
    chain.heartbeat(ttpb, Vec::new()).await;
    {
        // A self-consistent V4 spend (both signatures real) of a claim the chain does not hold: the header stage refuses it for the fence
        // before anything stateful is asked.
        let probe_auth = PalwRedemptionAuthV4 {
            version: PALW_RECEIPT_V4_VERSION,
            network_domain: domain,
            claim_id: h64(0xBE10),
            executor_bond: bonds[BUILDER_B].0,
            quantum_lo: 0,
            quantum_hi: 1,
            beacon_rule: PALW_RECEIPT_V4_BEACON_RULE_SLOT,
            builder_fee_bps: FEE_BPS,
            expiry_daa: u64::MAX,
        };
        let probe_bundle = PalwRedemptionAuthBundleV4 {
            signature: sign(BUILDER_B, redeem_auth_id_v4(&probe_auth).as_byte_slice(), PALW_RECEIPT_V4_AUTH_MLDSA87_CONTEXT),
            executor_pubkey: card_pubkey(BUILDER_B),
            authorization: probe_auth,
        };
        let mut probe = receipt_template(&chain, card_payout_spk(BUILDER_A), chain.ctx.simulated_time + 1, 0xB10F);
        sign_v4(&mut probe, domain, BUILDER_A, bonds[BUILDER_A].0, &probe_bundle, 0, h64(0xBEAC));
        let probe_daa = probe.header.daa_score;
        let probe_commitment = probe.header.palw_commitment.clone();
        assert!(probe_daa < H, "the probe stands below the fence ({probe_daa} < {H})");
        assert!(probe.header.palw_commitment.len() > PALW_COMMITMENT_MAX_BYTES, "a PFS4 carriage is above the 8,192-byte cap");
        let (_, verdict) = insert(&chain, probe).await;
        let why = verdict.expect_err("a PFS4 header below the fence is refused");
        // A-2 uniformity: refused as the live build refuses it — by the shape gate that build runs, which reads no `PFS4` form.
        let live = kaspa_consensus_core::pow_layer0::check_palw_commitment_shape_at(
            kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_RECEIPT_V3,
            &probe_commitment,
            false,
            kaspa_consensus_core::pow_layer0::PalwAttemptLaneV1::Unfenced,
        )
        .expect_err("the live build's gate refuses a PFS4 carriage");
        assert!(
            matches!(
                live,
                kaspa_consensus_core::pow_layer0::PowLayer0Error::PalwCommitmentTooLong { cap: PALW_COMMITMENT_MAX_BYTES, .. }
            ),
            "{live}"
        );
        assert!(why.contains(&live.to_string()), "refused below the fence as the live build refuses it: {why}");
        eprintln!("[rfc9-v4] 1. below the fence (DAA {probe_daa}): {why}");
    }

    // ---- 2a. the floor's free-prompt lane is published on chain (t46g: genesis certifies it in params, not in state) -------------------
    let backend = Base0Backend::new(
        misaka_palw_base0::classes::resolve_class_v1(
            &bundle.court,
            floor,
            chain.tip_state().1.class(&floor).expect("the floor is registered").artifact_root,
            &[],
        )
        .expect("the floor resolves from its registered root"),
    )
    .with_step_ladder_cap(bundle.court.max_step_leaf_count())
    .with_prompt_ids_form(config.params.palw_prompt_ids_form_v1());
    {
        let evidence = misaka_palw_base0::e2e_drill::rc_free_prompt_evidence_v1(misaka_palw_base0::e2e_drill::PalwRcFamilyV1::Base0)
            .expect("the floor drills its free-prompt lane");
        let family = Obj::FamilyCertified {
            evidence: Box::new(kaspa_consensus_core::palw_state_v2::PalwCertificationEvidenceV1::FreePrompt(evidence)),
        };
        let lane = Obj::ClassLaneCertified {
            class_id: floor,
            lane: kaspa_consensus_core::palw_state_v2::PalwCertifiedLaneV1::FreePrompt,
            profile: Box::new(backend.profile().clone()),
        };
        // The family's drill evidence is above one block's mass: it rides as `ObjectChunk`s (ADR-0075 Decision 14), one carrier per
        // block, each paying the group's slot rent and the certification's grading rent on top of its carriage; the completing chunk
        // applies the `FamilyCertified`. Then the class's lane binding, on one carrier.
        let rent = kaspa_consensus_core::palw_state_v2::palw_object_rent_ceiling_v1(&family);
        let chunks = kaspa_consensus_core::palw_state_v2::palw_object_chunks_v1(&family)
            .expect("the evidence chunks")
            .expect("the evidence is above one carrier");
        eprintln!("[rfc9-v4] 2a. the floor's FP family rides {} chunks (rent {rent})", chunks.len());
        let payers = [CERTIFIER, 0, 2, 4, 6, 7];
        let mut objects: Vec<(usize, Obj, u64)> = chunks
            .into_iter()
            .enumerate()
            .map(|(i, chunk)| {
                let fee = CARRIER_FEE + rent + kaspa_consensus_core::palw_state_v2::palw_object_rent_ceiling_v1(&chunk);
                (payers[i % payers.len()], chunk, fee)
            })
            .collect();
        objects.push((CERTIFIER, lane, CARRIER_FEE));
        for (i, (payer, object, fee)) in objects.into_iter().enumerate() {
            if i > 0 && i % payers.len() == 0 {
                chain.heartbeat(ttpb, Vec::new()).await;
            }
            let tx = wallets.carrier_paying(
                payer,
                kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
                lifecycle_payload(object),
                fee,
                &config,
            );
            let carrying = chain.heartbeat(ttpb, vec![tx.clone()]).await;
            assert!(carrying.transactions.iter().any(|t| t.id() == tx.id()), "the certification carrier is in the block");
        }
        chain.heartbeat(ttpb, Vec::new()).await;
        assert!(chain.tip_state().1.fp_work_profile_of(&floor).is_some(), "the floor's free-prompt graph is published on chain");
    }

    // ---- 2b. the executor: run, publish, claim, authorize — then offline --------------------------------------------------------------
    let left_behind = {
        use kaspa_consensus_core::palw_evidence_v1::EvidenceManifestV1;
        use kaspa_consensus_core::palw_fp_execution_v3::palw_fp_commitment_from_context_v3;
        use kaspa_consensus_core::palw_freeprompt_v3::{
            PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_PROMPT_MODE_USER, PALW_FP_V3_MLDSA87_COMMITMENT_CONTEXT, PALW_FP_V3_VERSION,
            PalwFpCommitmentTxPayloadV3, PalwFreePromptJobV3, fp_claim_id_v3, palw_fp_capture_encode_v1,
        };
        let executor_bond = bonds[EXECUTOR].0;
        let facts = chain.ctx.consensus.palw_producer_facts_v2(floor, Some(executor_bond)).expect("a V2 network answers");
        let operator_id = facts.bond.as_ref().expect("a genesis card is a bond").operator_id;
        let sink = chain.sink();
        let daa = chain.daa_of(sink);
        let ids: Vec<u32> = vec![3, 5, 8, 13];
        let prompt: Vec<usize> = ids.iter().map(|t| *t as usize).collect();
        let n_ctx = backend.profile().n_ctx;
        let job = PalwFreePromptJobV3 {
            version: PALW_FP_V3_VERSION,
            network_domain: domain,
            class_id: floor,
            executor_bond,
            executor_pubkey: card_pubkey(EXECUTOR),
            operator_id,
            anchor_block: sink,
            anchor_daa: daa,
            job_nonce: [0x49; 32],
            tokenizer_id: Hash64::default(),
            prompt_token_ids_hash: kaspa_consensus_core::palw_prompt_ids_v1::prompt_token_ids_commitment_v1(
                backend.prompt_ids_form(),
                &ids,
            )
            .expect("a short prompt commits"),
            prompt_tokens: ids.len() as u32,
            decode_token_limit: n_ctx - ids.len() as u32,
            max_context_tokens: n_ctx,
            privacy_mode: PALW_FP_PRIVACY_PUBLIC_DA,
            prompt_mode: PALW_FP_PROMPT_MODE_USER,
            sampling_seed: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_SEED_GREEDY,
            temperature_q: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_TEMPERATURE_GREEDY,
            decode: None,
            tail: None,
        };
        let run = backend.execute_free_prompt(&job, &prompt).expect("the floor runs a caller's prompt");
        let context = backend.capture_shape(&run.outcome.material).expect("the capture has a shape").job_context;
        let commitment = palw_fp_commitment_from_context_v3(&job, &context, &run, daa + facts.min_trace_retention_daa)
            .expect("the run becomes a commitment");
        let claim_id = fp_claim_id_v3(&commitment);

        // The material, to the independent providers, read back verified — BEFORE the claim goes on chain.
        let material = palw_fp_capture_encode_v1(&job, &ids, &run.outcome.material);
        let chunks = misaka_palw_remote::evidence::fs::chunk_material(&material, 64 << 10);
        let manifest = EvidenceManifestV1::build(
            domain,
            &executor_bond,
            &job.job_nonce,
            commitment.trace_root,
            commitment.output_root,
            commitment.execution_root,
            commitment.trace_chunk_count,
            // The chain dates the obligation from the claim's acceptance, not the job's anchor: promise a margin past it.
            commitment.trace_retention_daa + 1_000,
            &chunks,
        );
        let report = publish_to_providers_v1(&claim_id.to_string(), &manifest, &chunks, &provs, 2)
            .expect("both providers hold a verified copy");
        assert_eq!(report.verified_copies(), 2);

        // The claim, on a 0x4a carrier funded from the executor's own float.
        let payload = PalwFpCommitmentTxPayloadV3 {
            version: PALW_FP_V3_VERSION,
            commitment: commitment.clone(),
            prompt_token_ids: ids.clone(),
            signature: sign(EXECUTOR, claim_id.as_byte_slice(), PALW_FP_V3_MLDSA87_COMMITMENT_CONTEXT),
        };
        let tx = wallets.carrier(
            EXECUTOR,
            kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_FP_COMMITMENT,
            borsh::to_vec(&payload).expect("serializes"),
            &config,
        );
        let carrying = chain.heartbeat(ttpb, vec![tx.clone()]).await;
        assert!(carrying.transactions.iter().any(|t| t.id() == tx.id()), "the FP carrier is in the block");
        chain.heartbeat(ttpb, Vec::new()).await;
        let (_, state) = chain.tip_state();
        let claim = state.claim(&claim_id).expect("the commitment opened a claim on chain");
        let PalwClaimSourceV2::FreePrompt { quanta, .. } = &claim.source else { panic!("a free-prompt claim") };
        assert!(matches!(claim.phase, PalwClaimPhaseV2::Provisional), "{:?}", claim.phase);
        assert_eq!(claim.bond, PalwBondKeyV2(executor_bond));
        let quanta = *quanta;
        eprintln!(
            "[rfc9-v4] 2. claim {claim_id} opened at DAA {} with {quanta} quanta ({} work leaves)",
            claim.accepted_daa, commitment.work_leaves
        );

        // ONE position-free authorization: any bonded builder, every quantum, the beacon RULE, 500 bps, an expiry past the use window.
        let authorization = PalwRedemptionAuthV4 {
            version: PALW_RECEIPT_V4_VERSION,
            network_domain: domain,
            claim_id,
            executor_bond,
            quantum_lo: 0,
            quantum_hi: quanta,
            beacon_rule: PALW_RECEIPT_V4_BEACON_RULE_SLOT,
            builder_fee_bps: FEE_BPS,
            expiry_daa: daa + 10_000,
        };
        let rda4 = PalwRedemptionAuthBundleV4 {
            signature: sign(EXECUTOR, redeem_auth_id_v4(&authorization).as_byte_slice(), PALW_RECEIPT_V4_AUTH_MLDSA87_CONTEXT),
            executor_pubkey: card_pubkey(EXECUTOR),
            authorization,
        }
        .encode();
        publish_redemption_v1(&rda4, &provs, 2).expect("both providers hold the authorization");
        LeftBehind { claim_id, executor_bond }
    };
    // **The executor is gone.** From here nothing reads its key, its run or its machine; what is left is the chain and the providers.
    let LeftBehind { claim_id, executor_bond } = left_behind;

    // ---- 3. the chain binds the panel at the anchor slot ------------------------------------------------------------------------------
    // The binding attempt (never the executor's); which block the chain anchors the seats on is the anchor-window rule's.
    chain.attempt_at_the_anchor_slot(claim_id, BUILDER_C).await;
    let (_, state) = chain.tip_state();
    assert!(
        matches!(state.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::PanelBound { .. }),
        "{:?}",
        state.claim(&claim_id).unwrap().phase
    );
    let panel = state.panel(&claim_id).expect("bound").clone();
    let seat_cards: Vec<usize> =
        panel.seats.iter().map(|s| bonds.iter().position(|b| *b == s.bond).expect("a genesis card")).collect();
    assert!(!seat_cards.contains(&EXECUTOR), "the executor never sits on its own panel");
    eprintln!("[rfc9-v4] 3. panel bound: seats are cards {seat_cards:?}");

    // ---- 4. each seat verifies from the PROVIDERS only, then signs Valid; the node's assembler; a funded 0x4b carrier -----------------
    {
        use kaspa_consensus_core::palw_evidence_v1::{ClaimRoots, ManifestLimits};
        use kaspa_consensus_core::palw_panel_v2::{
            PALW_RECEIPT_V2_MLDSA87_CONTEXT, PalwReceiptVerdictV2, PalwSeatReceiptV2, palw_receipt_message_v2,
        };
        let (_, state) = chain.tip_state();
        let claim = state.claim(&claim_id).unwrap().clone();
        let on_chain = ClaimRoots {
            network_domain: domain,
            trace_root: claim.trace_root,
            output_root: claim.output_root,
            execution_root: claim.execution_root,
            trace_chunk_count: claim.trace_chunk_count,
            retention_deadline: claim.trace_retention_daa,
        };
        let signed_daa = chain.ctx.consensus.get_virtual_daa_score();
        let mut receipts = Vec::new();
        for (seat, card) in panel.seats.iter().zip(&seat_cards).take(bundle.panel.quorum() as usize) {
            // A seat's own provider list (each opens its own connections): nothing from the executor.
            let mine = open_providers_v1(&specs);
            let refs: Vec<&dyn EvidenceProvider> = mine.iter().map(|p| p.as_ref()).collect();
            let (bytes, _) =
                fetch_claim_material_any(&refs, &claim_id.to_string(), &on_chain, &ManifestLimits::default(), h64(*card as u64))
                    .expect("the providers serve the claim's material");
            let capture = kaspa_consensus_core::palw_freeprompt_v3::palw_fp_capture_decode_v1(&bytes, backend.prompt_ids_form())
                .expect("the material is the claim's FP capture");
            let roots = PalwClaimRootsV1 {
                execution_root: claim.execution_root,
                trace_root: claim.trace_root,
                anchor: fp_job_id_v3(&capture.material.job),
                attempt_draw: None,
                output_root: Some(claim.output_root),
                job_pin: Some(claim.job_identity),
            };
            assert_eq!(
                backend.verify_material(&capture.capture, roots),
                PalwMaterialVerdictV1::Matches,
                "seat card {card}: the material answers for the claim"
            );
            let message = palw_receipt_message_v2(domain, claim_id, PalwReceiptVerdictV2::Valid, signed_daa);
            receipts.push(PalwSeatReceiptV2 {
                claim: claim_id,
                verdict: PalwReceiptVerdictV2::Valid,
                seat_bond: seat.bond,
                signed_daa,
                signature: sign(*card, message.as_byte_slice(), PALW_RECEIPT_V2_MLDSA87_CONTEXT),
            });
        }
        let object = chain.vp().palw_v2_receipt_quorum_assemble_impl(claim_id, &receipts).expect("a signed quorum assembles");
        assert!(matches!(object, Obj::ReceiptLicensed { .. }), "a Valid quorum licenses the claim: {object:?}");
        let tx = wallets.carrier(
            seat_cards[0],
            kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
            lifecycle_payload(object),
            &config,
        );
        chain.heartbeat(ttpb, vec![tx]).await;
        chain.heartbeat(ttpb, Vec::new()).await;
        let (_, state) = chain.tip_state();
        assert!(
            matches!(state.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }),
            "{:?}",
            state.claim(&claim_id).unwrap().phase
        );
    }

    // ---- 5. Final ---------------------------------------------------------------------------------------------------------------------
    let final_daa = loop {
        if let PalwClaimPhaseV2::Final { final_daa } = chain.tip_state().1.claim(&claim_id).unwrap().phase {
            break final_daa;
        }
        assert!(chain.daa_of(chain.sink()) < 4_000, "the claim reaches Final");
        chain.heartbeat(ttpb, Vec::new()).await;
    };
    let slot = final_daa + maturity;
    eprintln!("[rfc9-v4] 5. Final at DAA {final_daa}; the draw slot is {slot}");

    // ---- 6. the draw slot; attempt blocks (never the executor's) carry the beacon ----------------------------------------------------
    while chain.daa_of(chain.sink()) < slot {
        chain.heartbeat(ttpb, Vec::new()).await;
    }
    let mut turn = 0usize;
    let beacon = loop {
        let card = [0usize, 1, 2, 4, 5, 6, 7][turn % 7];
        turn += 1;
        let (_, _) = chain.attempt(card, ttpb, Vec::new(), &|_| true).await;
        if let Ok(beacon) = chain.vp().palw_beacon_fact_of_candidate(chain.sink(), slot) {
            break beacon;
        }
        assert!(turn < 64, "the beacon derives once attempt blocks stand past the slot");
    };
    let wins: Vec<PalwFpSpendableQuantumV3> =
        chain.ctx.consensus.palw_fp_spendable_v3(executor_bond).into_iter().filter(|q| q.claim_id == claim_id && q.wins).collect();
    eprintln!(
        "[rfc9-v4] 6. beacon {} at DAA {}: winning quanta {:?} (target {:?})",
        beacon.beacon_block,
        beacon.beacon_daa,
        wins.iter().map(|q| q.quantum_index).collect::<Vec<_>>(),
        wins.first().map(|q| q.receipt_target)
    );
    assert!(!wins.is_empty(), "a quantum of the claim wins under this beacon (the harness does not grind it)");
    let quantum = wins[0].quantum_index;
    // P, the receipt blocks' parent, is a heartbeat (its reward goes to nobody this test measures).
    chain.heartbeat(ttpb, Vec::new()).await;

    // ---- 7. builder A redeems from the providers alone; a heartbeat merges the receipt block ------------------------------------------
    let fork = chain.sink(); // P: the receipt blocks' parent, and the reorg's fork point
    let sim_at_fork = chain.ctx.simulated_time;
    let dir_a = scratch("builder-a");
    let sync = sync_redemptions_v1(&provs, &dir_a, Some(domain), Some(chain.ctx.consensus.get_virtual_daa_score()))
        .expect("the builder's directory");
    assert_eq!(sync.written.len(), 1, "builder A mirrors the one authorization: {sync:?}");
    let (bundle_a, win) = builder_mode_pick(&chain, &dir_a, domain).expect("builder mode finds the winning, authorized quantum");
    assert_eq!((win.claim_id, win.quantum_index), (claim_id, quantum));
    let r_ts = chain.ctx.simulated_time + 1;
    let mut r1 = receipt_template(&chain, card_payout_spk(BUILDER_A), r_ts, 0xA1);
    sign_v4(&mut r1, domain, BUILDER_A, bonds[BUILDER_A].0, &bundle_a, quantum, win.beacon.beacon_block);
    // The sibling builder B builds at the same moment on the same parent; it is sent one block later (step 9).
    let dir_b = scratch("builder-b");
    sync_redemptions_v1(&provs, &dir_b, Some(domain), None).expect("builder B's directory");
    let (bundle_b, _) = builder_mode_pick(&chain, &dir_b, domain).expect("builder B finds it too");
    let mut r2 = receipt_template(&chain, card_payout_spk(BUILDER_B), r_ts + 1, 0xB2);
    sign_v4(&mut r2, domain, BUILDER_B, bonds[BUILDER_B].0, &bundle_b, quantum, win.beacon.beacon_block);
    assert!(r1.header.daa_score >= H, "the redemption stands past the fence");
    assert!(
        r1.header.palw_commitment.len() > PALW_COMMITMENT_MAX_BYTES && r1.header.palw_commitment.len() <= PALW_COMMITMENT_MAX_BYTES_V4
    );
    let (_, before_m) = chain.tip_state();
    assert!(fp_spent(&before_m, &claim_id).is_empty(), "at the fork the quantum is unspent");
    let (r1, v1) = insert(&chain, r1).await;
    v1.unwrap_or_else(|e| panic!("builder A's receipt block is accepted: {e}"));
    // A receipt block is a block of the chain like any other: it takes the tip (its own fold applies the spend) or is merged by the next.
    let r1_took_the_tip = chain.sink() == r1.header.hash;
    let m_spk = tagged_spk(0x4D);
    let m = chain.heartbeat_paying(MinerData::new(m_spk.clone(), vec![]), ttpb, Vec::new()).await;
    let m_gd = chain.vp().ghostdag_store.get_data(m.header.hash).expect("ghostdag");
    assert!(
        m_gd.selected_parent == r1.header.hash || m_gd.unordered_mergeset_without_selected_parent().any(|b| b == r1.header.hash),
        "the heartbeat's coinbase is the one that pays builder A's receipt block"
    );
    eprintln!("[rfc9-v4] 7. builder A's receipt block {} took the tip: {r1_took_the_tip}", r1.header.hash);
    let (_, after_m) = chain.tip_state();
    assert_eq!(fp_spent(&after_m, &claim_id), vec![quantum], "the merging chain block folds the spend");
    assert!(after_m.safe_weight() > before_m.safe_weight(), "and credits its weight");

    // ---- 7b. the coinbase: the miner leg to the executor's registered payout, the fee to builder A -------------------------------------
    // The receipt block's subsidy-derived worker reward, as the coinbase manager splits it at the block's own DAA.
    let part = {
        let vp = chain.vp();
        let subsidy = vp.coinbase_manager.calc_block_subsidy(r1.header.daa_score);
        match vp.fee_split_at(r1.header.daa_score) {
            Some(split) => kaspa_consensus_core::dns_finality::split_block_subsidy(subsidy, &split).worker_base_sompi,
            None => subsidy,
        }
    };
    let (leg, fee) = palw_receipt_v4_split_v1(part, FEE_BPS);
    assert!(leg > 0 && fee > 0, "both legs are positive (leg {leg}, fee {fee} of {part})");
    assert!(
        u128::from(fee) * 10_000 <= u128::from(part) * u128::from(PALW_RECEIPT_V4_BUILDER_FEE_CAP_BPS),
        "the fee is within the chain's cap"
    );
    assert_eq!(paid(&m, &card_payout_spk(BUILDER_A)), fee, "builder A's script is paid exactly the authorization's fee");
    assert!(paid(&m, &executor_spk) >= leg, "the executor bond's registered payout is paid the miner leg");
    eprintln!(
        "[rfc9-v4] 7. builder A's {} merged by {}: executor leg {leg}, builder fee {fee} (of the worker reward {part})",
        r1.header.hash, m.header.hash
    );

    // ---- 8. the V3 counterfactual twin: the executor online, spending the same quantum itself at the same position ---------------------
    {
        let mut twin = t12_genesis_chain(&config, &bundle, &premine, &floats);
        feed(&twin, &past_in_order(&chain, fork), "the twin's replay").await;
        assert_eq!(twin.sink(), fork);
        twin.ctx.simulated_time = sim_at_fork;
        // (The counterfactual is the only place the executor's key is used after step 2: what V3 would have paid had it stayed online.)
        let mut v3 = receipt_template(&twin, executor_spk.clone(), r_ts, 0xA1);
        sign_v3(&mut v3, domain, EXECUTOR, executor_bond, claim_id, quantum, win.beacon.beacon_block);
        let (_, v) = insert(&twin, v3).await;
        v.unwrap_or_else(|e| panic!("the V3 twin's receipt block is accepted: {e}"));
        let m3 = twin.heartbeat_paying(MinerData::new(m_spk.clone(), vec![]), ttpb, Vec::new()).await;
        assert_eq!(m3.header.daa_score, m.header.daa_score, "the twin's merging block stands at the same DAA");
        let (v4_map, v3_map) = (by_script(&m), by_script(&m3));
        assert_eq!(v4_map.values().sum::<u64>(), v3_map.values().sum::<u64>(), "no value is minted or burned by the split");
        let get = |map: &BTreeMap<Vec<u8>, u64>, spk: &ScriptPublicKey| map.get(spk.script()).copied().unwrap_or(0);
        assert_eq!(get(&v3_map, &executor_spk) - get(&v4_map, &executor_spk), fee, "V4 pays the executor V3's amount less the fee");
        assert_eq!(
            get(&v4_map, &card_payout_spk(BUILDER_A)) - get(&v3_map, &card_payout_spk(BUILDER_A)),
            fee,
            "…and the builder the fee"
        );
        let mut folded_back = v4_map.clone();
        *folded_back.get_mut(card_payout_spk(BUILDER_A).script()).unwrap() -= fee;
        folded_back.retain(|_, v| *v > 0);
        *folded_back.entry(executor_spk.script().to_vec()).or_default() += fee;
        assert_eq!(
            folded_back, v3_map,
            "moving the fee back to the executor reproduces V3's coinbase: Panel leg, reserve, every other output"
        );
        let (_, twin_state) = twin.tip_state();
        assert_eq!(twin_state.safe_weight(), after_m.safe_weight(), "the same weight");
        assert_eq!(twin_state.claim(&claim_id), after_m.claim(&claim_id), "the same claim row: who built the block is a payout fact");
        eprintln!("[rfc9-v4] 8. V3 twin: totals equal; the only difference is the {fee}-sompi fee carve");
    }

    // ---- 9. single use: the sibling (builder B, same parent P) arrives late; a V3 re-spend after both ---------------------------------
    // The two siblings are the same PALW state (the builder is a payout fact), so the fork choice may keep A's branch or take B's on the
    // hash: either way the selected chain pays ONE fee and ONE miner leg for the quantum, and the other spend is unentitled.
    {
        let (r2, v2) = insert(&chain, r2).await;
        v2.unwrap_or_else(|e| panic!("the sibling is a well-formed block: {e}"));
        let r2_took_the_tip = chain.sink() == r2.header.hash;
        chain.heartbeat_paying(MinerData::new(tagged_spk(0x4D02), vec![]), ttpb, Vec::new()).await;
        assert_eq!(fp_spent(&chain.tip_state().1, &claim_id), vec![quantum], "one spend of the quantum on the selected chain");
        let (fee_a, fee_b) = (
            paid_on_chain_since(&chain, fork, &card_payout_spk(BUILDER_A)),
            paid_on_chain_since(&chain, fork, &card_payout_spk(BUILDER_B)),
        );
        assert_eq!(
            (fee_a == fee) as u8 + (fee_b == fee) as u8,
            1,
            "exactly one sibling's builder is paid the fee (A {fee_a}, B {fee_b})"
        );
        assert_eq!(fee_a + fee_b, fee, "and nothing else");
        assert_eq!(paid_on_chain_since(&chain, fork, &executor_spk), leg, "the executor's leg is paid once on the selected chain");
        eprintln!(
            "[rfc9-v4] 9. the late sibling took the tip: {r2_took_the_tip}; paid on the chain: A {fee_a}, B {fee_b}, executor {leg}"
        );

        // The executor's own V3 spend of the same quantum (its key is back, or anybody holding it): refused as a chain block, unpaid merged.
        let r3_spk = tagged_spk(0x3333);
        let mut r3 = receipt_template(&chain, r3_spk.clone(), chain.ctx.simulated_time + 1, 0xC3);
        sign_v3(&mut r3, domain, EXECUTOR, executor_bond, claim_id, quantum, win.beacon.beacon_block);
        let (r3, v3) = insert(&chain, r3).await;
        let r3_status = chain.ctx.consensus.block_status(r3.header.hash);
        eprintln!("[rfc9-v4] 9. the V3 re-spend {}: {v3:?}, {r3_status:?}", r3.header.hash);
        assert_ne!(chain.sink(), r3.header.hash, "a V3 re-spend cannot be a chain block: its own fold refuses the spent quantum");
        let m3 = chain.heartbeat_paying(MinerData::new(tagged_spk(0x4D03), vec![]), ttpb, Vec::new()).await;
        assert_eq!(paid(&m3, &r3_spk), 0, "a V3 spend of a quantum V4 already spent is paid nothing");
        assert_eq!(fp_spent(&chain.tip_state().1, &claim_id), vec![quantum]);
        assert_eq!(paid_on_chain_since(&chain, fork, &executor_spk), leg, "and pays the executor no second leg");
    }

    // ---- 10. reorg: a branch from P that the PALW fork choice prefers, which never saw the spend; then two builders race on it ---------
    // The canonical chain is the PALW candidate order (safe frontier, safe weight, live total, hash — `decide_deep_reorg_v2`), not the
    // header blue work: a branch of heartbeats a block or two longer than A's is REFUSED (it carries less matured weight than the
    // redemption). The branch that takes A carries MORE: two other winning quanta of the same claim redeemed by builder C — and in it, A's
    // quantum was never spent.
    let a_after_fork = chain_blocks_after(&chain, fork);
    let mut b = t12_genesis_chain(&config, &bundle, &premine, &floats);
    feed(&b, &past_in_order(&chain, fork), "B's replay to the fork").await;
    b.ctx.simulated_time = sim_at_fork;
    let mut longer = Vec::new();
    for _ in 0..(a_after_fork + 2) {
        longer.push(b.heartbeat(ttpb, Vec::new()).await);
    }
    feed(&chain, &longer, "A shown a longer spend-less branch").await;
    assert_ne!(chain.sink(), b.sink(), "a spend-less branch, longer in blue work, does not displace the redemption");
    let dir_c = scratch("builder-c");
    sync_redemptions_v1(&provs, &dir_c, Some(domain), None).expect("builder C's directory");
    let (bundle_c, _) = builder_mode_pick(&b, &dir_c, domain).expect("builder C finds the authorization redeemable on B");
    let others: Vec<PalwFpSpendableQuantumV3> = b
        .ctx
        .consensus
        .palw_fp_spendable_v3(executor_bond)
        .into_iter()
        .filter(|q| q.claim_id == claim_id && q.wins && q.quantum_index != quantum)
        .take(2)
        .collect();
    assert_eq!(others.len(), 2, "two other winning quanta of the claim (the harness does not grind the beacon)");
    let mut heavier = Vec::new();
    for (i, other) in others.iter().enumerate() {
        let mut r = receipt_template(&b, card_payout_spk(BUILDER_C), b.ctx.simulated_time + 1, 0xC0 + i as u64);
        sign_v4(&mut r, domain, BUILDER_C, bonds[BUILDER_C].0, &bundle_c, other.quantum_index, other.beacon.beacon_block);
        let (r, v) = insert(&b, r).await;
        v.unwrap_or_else(|e| panic!("builder C's redemption of quantum {} is accepted: {e}", other.quantum_index));
        heavier.push(r);
        heavier.push(b.heartbeat(ttpb, Vec::new()).await);
    }
    feed(&chain, &heavier, "A taking B's heavier branch").await;
    assert_eq!(chain.sink(), b.sink(), "A reorgs onto the branch with more matured weight");
    let mut on_b: Vec<u32> = others.iter().map(|q| q.quantum_index).collect();
    on_b.sort();
    assert_eq!(fp_spent(&chain.tip_state().1, &claim_id), on_b, "the reorg reverts A's spend: its quantum is unspent on the new tip");
    {
        let ts = b.ctx.simulated_time + 1;
        let mut r4 = receipt_template(&b, card_payout_spk(BUILDER_C), ts, 0xD4);
        sign_v4(&mut r4, domain, BUILDER_C, bonds[BUILDER_C].0, &bundle_c, quantum, win.beacon.beacon_block);
        let mut r5 = receipt_template(&b, card_payout_spk(BUILDER_B), ts + 1, 0xD5);
        sign_v4(&mut r5, domain, BUILDER_B, bonds[BUILDER_B].0, &bundle_c, quantum, win.beacon.beacon_block);
        let mut race = Vec::new();
        for r in [r4, r5] {
            let (r, v) = insert(&b, r).await;
            v.unwrap_or_else(|e| panic!("a racing receipt block is accepted: {e}"));
            race.push(r);
        }
        let mb = b.heartbeat_paying(MinerData::new(tagged_spk(0x4DB), vec![]), ttpb, Vec::new()).await;
        race.push(mb.clone());
        let (fee_c, fee_b) = (paid(&mb, &card_payout_spk(BUILDER_C)), paid(&mb, &card_payout_spk(BUILDER_B)));
        assert!(
            (fee_c == fee) ^ (fee_b == fee) && fee_c + fee_b == fee,
            "two spends of one quantum in one mergeset: exactly one builder is paid the fee (C {fee_c}, B {fee_b})"
        );
        assert_eq!(paid(&mb, &executor_spk), leg, "the executor is paid the quantum's leg once, on this branch");
        let mut all = on_b.clone();
        all.push(quantum);
        all.sort();
        assert_eq!(fp_spent(&b.tip_state().1, &claim_id), all, "A's quantum is spent again — once — on the new branch");
        feed(&chain, &race, "A following B").await;
        assert_eq!(chain.sink(), b.sink());
        assert_eq!(chain.tip_state().1.state_root(), b.tip_state().1.state_root(), "A and B fold the same root");
        eprintln!(
            "[rfc9-v4] 10. a longer spend-less branch refused; the heavier branch (quanta {on_b:?}) reverted the spend; the race on quantum \
             {quantum}: C {fee_c} / B {fee_b}"
        );
    }

    // ---- 11. a node replaying the winning branch from genesis (across H) agrees ----------------------------------------------------------
    {
        let z = t12_genesis_chain(&config, &bundle, &premine, &floats);
        feed(&z, &past_in_order(&b, b.sink()), "a fresh node's IBD").await;
        assert_eq!(z.sink(), b.sink(), "the replaying node reaches the same sink");
        assert_eq!(z.tip_state().1.state_root(), b.tip_state().1.state_root(), "and the same PALW root, the redemption in");
    }

    // ---- 12. past the fence, a PFS3 carriage above the 8,192-byte cap is still refused ------------------------------------------------
    {
        let mut big = receipt_template(&b, card_payout_spk(BUILDER_A), b.ctx.simulated_time + 1, 0xE1);
        sign_v3(&mut big, domain, BUILDER_A, bonds[BUILDER_A].0, claim_id, quantum, win.beacon.beacon_block);
        big.header.palw_commitment.resize(PALW_COMMITMENT_MAX_BYTES + 64, 0);
        big.header.finalize();
        let (_, verdict) = insert(&b, big).await;
        let why = verdict.expect_err("a PFS3 carriage above the cap is refused past the fence as before");
        eprintln!("[rfc9-v4] 12. an oversized PFS3 header past the fence: {why}");
    }
    http.stop();
    let _ = std::fs::remove_dir_all(&root);
}

/// **Every block in `upto`'s past, in an order a peer can take them**: the selected chain from genesis, each chain block preceded by its
/// mergeset in consensus (topological) order — what an IBD replays, side blocks (receipt blocks, merged reds) included.
fn past_in_order(chain: &T12Chain, upto: BlockHash) -> Vec<Block> {
    let vp = chain.vp();
    let genesis = chain.config.params.genesis.hash;
    let mut spine = Vec::new();
    let mut at = upto;
    while at != genesis {
        spine.push(at);
        at = vp.ghostdag_store.get_selected_parent(at).expect("a chain block has a selected parent");
    }
    spine.reverse();
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for hash in spine {
        let data = vp.ghostdag_store.get_data(hash).expect("ghostdag");
        for merged in data.consensus_ordered_mergeset_without_selected_parent(vp.ghostdag_store.as_ref()) {
            if merged != genesis && seen.insert(merged) {
                out.push(chain.ctx.consensus.get_block(merged).expect("the node holds its past"));
            }
        }
        if seen.insert(hash) {
            out.push(chain.ctx.consensus.get_block(hash).expect("the node holds its chain"));
        }
    }
    out
}

/// What the selected chain's coinbases paid `spk` above `fork` (a merged block's own coinbase is never accepted; the chain block's pays).
fn paid_on_chain_since(chain: &T12Chain, fork: BlockHash, spk: &ScriptPublicKey) -> u64 {
    let vp = chain.vp();
    let mut at = chain.sink();
    let mut total = 0u64;
    while at != fork {
        total += paid(&chain.ctx.consensus.get_block(at).expect("a chain block"), spk);
        at = vp.ghostdag_store.get_selected_parent(at).expect("a chain block has a selected parent");
    }
    total
}

/// How many chain blocks `chain`'s selected chain holds above `fork`.
fn chain_blocks_after(chain: &T12Chain, fork: BlockHash) -> usize {
    let vp = chain.vp();
    let mut at = chain.sink();
    let mut n = 0usize;
    while at != fork {
        n += 1;
        at = vp.ghostdag_store.get_selected_parent(at).expect("a chain block has a selected parent");
    }
    n
}
