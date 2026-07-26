//! ADR-0044 — the full-lifecycle long-chain PALW harness (keystone).
//!
//! This is the integration test `docs/palw-nullifier-lifecycle-audit.md:58` records as MISSING and
//! `docs/adr/0044-nullifier-prune-replay-e2e.md` specifies: build a REAL batch lifecycle **through
//! block acceptance** (provider bonds → manifest → leaf-chunk → attested certificate → algo-4 mint →
//! provider payment — NO hand-seeded stores, per ADR-0044 §Consequences), then pile plain blocks past
//! the (shrunk) pruning depth so the REAL pruning processor runs a REAL pass, and only then assert
//! the prune-then-replay contract:
//!
//!   1. per-block PALW nullifier rows below the pruning point are deleted (store audit);
//!   2. (2a) PRE-PASS baseline: a same-height reuse of a minted ticket is recolored red (credit
//!      denial) by the persisted nullifier window — the mechanism, on real minted tickets. (2b) Since
//!      `walk_bound < pruning_depth` puts the whole batch window BELOW the first pruning point (which
//!      tracks `sink − pruning_depth`), every minted block is pruned; the surviving reuse-catching
//!      object is the pruned FRONTIER's retention window (== the pp's persisted window), which still
//!      carries both minted nullifiers, so a joining node's re-import recolors a reuse of either red;
//!   3. a replay at a PRUNED height is structurally impossible: clause 5 pins the ticket to its
//!      `target_daa_interval == block DAA`, now buried below the pruning point, and the per-block
//!      windows across that region — including the replay's selected-parent seed — are gone, so a
//!      windowed same-height replay cannot be reconstructed locally (the audit-doc "clause-5
//!      coupling" row, asserted rather than merely recorded);
//!   4. beyond `palw_nullifier_retention_daa` the nullifier is OUT of every live window — window-exit
//!      is SPEC (nullifiers are epoch/batch-ground; out-of-window reuse ranks as a fresh ticket);
//!   5. G16 paid-work: the bounded walk's below-boundary rows are carried by the pruning-point
//!      snapshot across the pass (the job-nullifier dedup input survives pruning);
//!   6. ADR-0042 §1c: the transported payload derivations equal the live store derivations at the
//!      REAL pruning point of a lifecycle-coherent chain —
//!      `palw_pruning_payload_paid_work_nullifiers == palw_paid_work_window(pp)` and
//!      `palw_pruning_payload_da_state_root == palw_da_parent_state(pp).state_root()`.
//!
//! ## Why acceptance-path only
//!
//! The mint-env (`palw_algo4_env`) hand-seeds leaf/cert/view/bond stores. 2026-07-26 established that
//! such state is NOT coherent enough for the pruning snapshot builder (manifest/lifecycle binding
//! makes it correctly refuse hand-seeded state). Driving the SAME state through
//! `validate_and_insert_block` means the production builder's checks validate the fixture for free —
//! a fixture the builder rejects is a fixture bug, and a payload the verifier rejects is a builder
//! bug. Hand-reproducing the builder is forbidden (ADR-0042 §2): it cannot detect builder bugs.
//!
//! ## Parameter shape (and why each number)
//!
//! `epoch_len = 50`; admission `lead=2 / audit=1 / active=10` ⇒ `max_batch_life = 2·2+1+10 = 15` ⇒
//! G16 walk bound `(15+1)·50 = 800 < pruning_depth = 900` (the preset-pinned relation). EMPIRICAL:
//! the pruning point tracks `sink − pruning_depth` and its first advance jumps straight to
//! ~`0.9·pruning_depth` (≈ daa 810 here). Because `walk_bound < pruning_depth`, the ENTIRE batch
//! window is necessarily BELOW that first pp — no mint can be above it. Batch lifecycle: registration
//! epoch 0 → audit epoch 3 → activation epoch 4 (daa 200) → expiry epoch 14 (daa 700). W mints ≈ daa
//! 250, M1 ≈ daa 450, and the reuse-merger P_R ≈ daa 452 — all below the pp (~810), all pruned by the
//! pass. The pre-pass recolor of P_R's reuse is the mechanism baseline (2a); the post-pass survivor is
//! the FRONTIER (2b). `palw_nullifier_retention_daa = 1000` exceeds the pp-to-W gap (~560) so both
//! mints are carried in the frontier AT the pp, yet is smaller than the tip-to-mint gap (~1250) so
//! both leave every live window by the final tip (window-exit assert).

use std::collections::HashSet;
use std::time::Duration;

use kaspa_consensus_core::{
    api::ConsensusApi,
    block::MutableBlock,
    blockstatus::BlockStatus,
    coinbase::MinerData,
    config::ConfigBuilder,
    dns_finality::p2pkh_mldsa87_spk,
    hashing::sighash::{Mldsa87SigHashReusedValuesUnsync, calc_mldsa87_signature_hash},
    hashing::sighash_type::SIG_HASH_ALL,
    mass::MassCalculator,
    palw::{
        BeaconDnsAnchor, LaneDifficultyParams, PALW_AUDITOR_V2_MLDSA87_CONTEXT, PALW_BATCH_CERTIFICATE_VERSION_V2,
        PALW_LEAF_CHUNK_VERSION_V2, PALW_PAYLOAD_VERSION_V1, PalwAuditorVoteV2, PalwBatchCertificateV2, PalwBatchManifestV1,
        PalwLeafChunkV1, PalwProviderBondPayloadV1, PalwPublicLeafV1, ProviderBondView, chain_commit,
        dns_finality_certificate_hash_v1, eligibility_hash, palw_audit_sample_root, palw_deterministic_sample,
        palw_eligibility_win, palw_leaf_merkle_proof, palw_leaf_merkle_root, provider_bond_lock_spk,
        select_weighted_auditor_committee, ticket_nullifier_commitment,
    },
    subnets::{
        SUBNETWORK_ID_PALW_BATCH_CERT, SUBNETWORK_ID_PALW_BATCH_MANIFEST, SUBNETWORK_ID_PALW_LEAF_CHUNK,
        SUBNETWORK_ID_PALW_PROVIDER_BOND, SubnetworkId,
    },
    tx::{PopulatedTransaction, ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry},
};
use kaspa_consensus_core::config::params::{ForkActivation, SIMNET_PARAMS};
use kaspa_consensus_core::palw_pruned_frontier::{palw_pruning_payload_da_state_root, palw_pruning_payload_paid_work_nullifiers};
use kaspa_hashes::Hash64;
use kaspa_txscript::{MLDSA87_TX_CONTEXT, script_builder::ScriptBuilder};
use libcrux_ml_dsa::ml_dsa_87 as mldsa;

use crate::consensus::test_consensus::TestConsensus;
use crate::model::stores::ghostdag::GhostdagStoreReader;
use crate::model::stores::headers::HeaderStoreReader;
use crate::model::stores::palw::PalwStoreReader;
use crate::model::stores::palw_nullifier::PalwNullifierStoreReader;
use crate::model::stores::palw_provider_bonds::PalwProviderBondsStoreReader;
use crate::pipeline::virtual_processor::tests::{
    PALW_TEST_AUTHORITY_SEED, PalwAlgo4Facts, dns_harness, mint_algo4, palw_authority_pk_hash,
};
use crate::processes::palw::{resolve_palw_audit_epoch_seed, resolve_palw_lagged_anchor};

const EPOCH_LEN: u64 = 50;
/// The audit snapshot epoch. Its seed R_{AUDIT_EPOCH-1} must resolve, and `palw_epoch_seed_at`
/// fails closed on ANY zero-seed header on the descent — seeds turn non-zero only from the first
/// PALW epoch boundary after the DNS anchor confirms (the DNS stage below lands before daa 100),
/// so epoch 2 is the first fully non-zero epoch and the audit snapshot sits at epoch 3.
const AUDIT_EPOCH: u64 = 3;
const ACTIVATION_EPOCH: u64 = 4;
/// The batch's active window is epoch 4 → 14 (daa 200-700). All mints land inside it and, because
/// `walk_bound < pruning_depth`, all fall BELOW the first pruning point (which tracks
/// `sink − pruning_depth` ≈ 0.9·pruning_depth). walk_bound = (max_batch_life+1)·50 = (15+1)·50 = 800
/// < 900, and the active window is wide enough that pp − W (~560) stays inside both walk_bound and
/// retention with margin.
const EXPIRY_EPOCH: u64 = 14;
/// Pruning samples at finality multiples (300/600/900…). With PRUNING below, the first pass lands
/// at sample 300 (`sink − pruning_depth` rounded down), strictly between W (~250) and M1 (~400).
const FINALITY: u64 = 300;
/// pp = sink − PRUNING. Kept small so sample 300 is reachable while the batch (expiry daa 500) is
/// still active at M1's height. walk_bound = (max_batch_life+1)·50 = (11+1)·50 = 600 < 900 (the
/// preset-pinned relation).
const PRUNING: u64 = 900;
/// > the pp-to-W gap (~560) so W stays inside retention AT the pp (carried by the frontier), yet
/// < the tip-to-mint gap (~1250) so both mints have left every live window by the final tip.
const RETENTION: u64 = 1000;
/// econ03's floor shape: over the KIP-9 storage-mass knee, under one SIMNET coinbase.
const BOND_SOMPI: u64 = 20_000_000;
const CARRIER_FEE: u64 = 100_000;

/// A distinct, collision-free hash for the i-th harness-built block. The le-counter + 0xA5 filler
/// pattern can never equal the `[b; 64]` repeated-byte hashes `mint_algo4` derives from its `seed`
/// byte, so the two namespaces cannot collide.
fn hash_at(i: u64) -> Hash64 {
    let mut bytes = [0xA5u8; 64];
    bytes[..8].copy_from_slice(&i.to_le_bytes());
    Hash64::from_bytes(bytes)
}

fn plain_p2pkh_spk(pubkey: &[u8]) -> ScriptPublicKey {
    let payload: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(pubkey).as_bytes();
    p2pkh_mldsa87_spk(&payload)
}

/// Sign input-0 of a funded carrier/bond tx with the seed's ML-DSA-87 key (the dns_harness shape).
fn sign_input0(tx: &mut Transaction, seed: [u8; 32], utxo: UtxoEntry) {
    let kp = mldsa::generate_key_pair(seed);
    let pubkey = kp.verification_key.as_ref().to_vec();
    let reused = Mldsa87SigHashReusedValuesUnsync::new();
    let sig_hash = {
        let populated = PopulatedTransaction::new(tx, vec![utxo]);
        calc_mldsa87_signature_hash(&populated, 0, SIG_HASH_ALL, &reused)
    };
    let sig = mldsa::sign(&kp.signing_key, sig_hash.as_bytes().as_slice(), MLDSA87_TX_CONTEXT, [0x77u8; 32]).expect("mldsa sign");
    let mut sig_item = sig.as_ref().to_vec();
    sig_item.push(SIG_HASH_ALL.to_u8());
    tx.inputs[0].signature_script =
        ScriptBuilder::new().add_data(&sig_item).expect("sig push").add_data(&pubkey).expect("pk push").drain();
}

fn commit_storage_mass(tx: &mut Transaction, utxo: &UtxoEntry, storage_mass_parameter: u64) {
    let storage_mass = MassCalculator::new(0, 0, 0, storage_mass_parameter)
        .calc_contextual_masses(&PopulatedTransaction::new(tx, vec![utxo.clone()]))
        .expect("contextual mass computable")
        .storage_mass;
    tx.set_mass(storage_mass);
}

/// A funded, signed provider-bond tx (0x30) with a caller-chosen `operator_group_id`.
///
/// NOT `dns_harness::funded_signed_provider_bond_tx`: that helper hardcodes one operator group for
/// every bond, and AUTHSET-01 excludes the batch providers' operator-group SIBLINGS from the auditor
/// pool — with a shared group the exclusion would swallow the auditors too and the candidate pool
/// would be empty. Auditor eligibility is exactly what this harness must keep honest, so each bond
/// carries its own group.
fn funded_bond_tx(
    seed: [u8; 32],
    funding: (TransactionOutpoint, u64, u64),
    operator_group_id: Hash64,
    storage_mass_parameter: u64,
) -> Transaction {
    let kp = mldsa::generate_key_pair(seed);
    let pubkey = kp.verification_key.as_ref().to_vec();
    let payload = PalwProviderBondPayloadV1 {
        version: PALW_PAYLOAD_VERSION_V1,
        owner_public_key: pubkey.clone(),
        operator_group_id,
        runtime_classes: vec![Hash64::from_bytes([0x32; 64])],
        capacity_by_shape: vec![(1, 10)],
        reward_key_root: Hash64::from_bytes([0x33; 64]),
        amount_sompi: BOND_SOMPI,
        unbond_delay_epochs: 6,
    };
    let (outpoint, value, daa) = funding;
    let mut tx = Transaction::new(
        crate::constants::TX_VERSION,
        vec![TransactionInput::new(outpoint, vec![], 0, 1)],
        vec![TransactionOutput::new(BOND_SOMPI, provider_bond_lock_spk(&pubkey))],
        0,
        SUBNETWORK_ID_PALW_PROVIDER_BOND,
        0,
        borsh::to_vec(&payload).unwrap(),
    );
    let utxo = UtxoEntry::new(value, plain_p2pkh_spk(&pubkey), daa, true);
    commit_storage_mass(&mut tx, &utxo, storage_mass_parameter);
    sign_input0(&mut tx, seed, utxo);
    tx
}

/// A funded, signed PALW overlay carrier (manifest / leaf-chunk / certificate). Overlay carriers are
/// ordinary funded txs — a zero-input carrier dies in tx isolation — so the batch lifecycle rides the
/// same acceptance path any user transaction does.
fn funded_overlay_tx(
    seed: [u8; 32],
    funding: (TransactionOutpoint, u64, u64),
    subnet: SubnetworkId,
    payload: Vec<u8>,
    storage_mass_parameter: u64,
) -> Transaction {
    let kp = mldsa::generate_key_pair(seed);
    let pubkey = kp.verification_key.as_ref().to_vec();
    let spk = plain_p2pkh_spk(&pubkey);
    let (outpoint, value, daa) = funding;
    let mut tx = Transaction::new(
        crate::constants::TX_VERSION,
        vec![TransactionInput::new(outpoint, vec![], 0, 1)],
        vec![TransactionOutput::new(value - CARRIER_FEE, spk.clone())],
        0,
        subnet,
        0,
        payload,
    );
    let utxo = UtxoEntry::new(value, spk, daa, true);
    commit_storage_mass(&mut tx, &utxo, storage_mass_parameter);
    sign_input0(&mut tx, seed, utxo);
    tx
}


/// Continuous beacon driver: PALW activation (`Certified → Active`) is gated on the LAGGED beacon
/// window looking Healthy, i.e. the epoch seed must keep ADVANCING — a single Healthy round decays
/// into DegradedGrace carries and the gate never opens (the live-devnet "beacon must outpace the
/// epoch" lesson, reproduced here). The driver keeps the commit(E@E-2)/reveal(E@E-1) cadence going
/// through the mint window, funded by one wallet chained through change outputs.
struct BeaconDriver {
    bond: TransactionOutpoint,
    /// signs the beacon payloads (the DNS validator's key — the bond's registered pubkey)
    seed: [u8; 32],
    /// signs the funding inputs (the carrier wallet's key)
    wallet_seed: [u8; 32],
    wallet: (TransactionOutpoint, u64, u64),
    /// highest epoch a commit was sent for
    committed_target: u64,
    /// highest epoch a reveal was sent for
    revealed_target: u64,
    /// stop driving beyond this target epoch
    max_target: u64,
}

impl BeaconDriver {
    fn random_for(target: u64) -> [u8; 64] {
        let mut r = [0xE7u8; 64];
        r[0] = target as u8;
        r
    }

    /// Send the commit/reveal due at the CURRENT epoch, if any. Returns whether a carrier was sent
    /// (the caller then owns advancing the chain further).
    async fn tick(&mut self, chain: &mut Chain, net_id: u32, storage_mass_parameter: u64) {
        use kaspa_consensus_core::palw::{PALW_BEACON_MLDSA87_CONTEXT, PalwBeaconCommitV1, PalwBeaconRevealV1, beacon_commitment};
        let kp = mldsa::generate_key_pair(self.seed);
        // The NEXT block carries the tx; phase predicates run at its ACCEPTING chain block (tip+2).
        let carrier_epoch = (chain.tip_daa + 2) / EPOCH_LEN;
        let commit_target = carrier_epoch + 2;
        if commit_target <= self.max_target && self.committed_target < commit_target {
            let random = Self::random_for(commit_target);
            let mut commit = PalwBeaconCommitV1 {
                version: PALW_PAYLOAD_VERSION_V1,
                epoch: commit_target,
                bond_outpoint: self.bond,
                commitment: beacon_commitment(commit_target, &random, &self.bond),
                signature: vec![],
            };
            let cd = commit.signing_hash(net_id);
            commit.signature = mldsa::sign(&kp.signing_key, cd.as_bytes().as_slice(), PALW_BEACON_MLDSA87_CONTEXT, [0x11; 32])
                .expect("sign beacon commit")
                .as_ref()
                .to_vec();
            let tx = funded_overlay_tx(
                self.wallet_seed,
                self.wallet,
                kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_BEACON_COMMIT,
                borsh::to_vec(&commit).unwrap(),
                storage_mass_parameter,
            );
            self.wallet = (TransactionOutpoint::new(tx.id(), 0), self.wallet.1 - CARRIER_FEE, chain.tip_daa + 1);
            chain.extend_with(vec![tx], None).await;
            self.committed_target = commit_target;
        }
        let reveal_target = (chain.tip_daa + 2) / EPOCH_LEN + 1;
        if reveal_target <= self.committed_target && self.revealed_target < reveal_target {
            let random = Self::random_for(reveal_target);
            let mut reveal = PalwBeaconRevealV1 {
                version: PALW_PAYLOAD_VERSION_V1,
                epoch: reveal_target,
                bond_outpoint: self.bond,
                random_64: random,
                signature: vec![],
            };
            let rd = reveal.signing_hash(net_id);
            reveal.signature = mldsa::sign(&kp.signing_key, rd.as_bytes().as_slice(), PALW_BEACON_MLDSA87_CONTEXT, [0x12; 32])
                .expect("sign beacon reveal")
                .as_ref()
                .to_vec();
            let tx = funded_overlay_tx(
                self.wallet_seed,
                self.wallet,
                kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_BEACON_REVEAL,
                borsh::to_vec(&reveal).unwrap(),
                storage_mass_parameter,
            );
            self.wallet = (TransactionOutpoint::new(tx.id(), 0), self.wallet.1 - CARRIER_FEE, chain.tip_daa + 1);
            chain.extend_with(vec![tx], None).await;
            self.revealed_target = reveal_target;
        }
    }
}

/// The harness chain: strictly LINEAR (daa == height on this shape, and width-1 keeps the pruned
/// region free of merge points), with the coinbase of every built block captured for assertions.
struct Chain {
    tc: TestConsensus,
    tip: Hash64,
    tip_daa: u64,
    counter: u64,
    miner: MinerData,
    /// FIRST chain block observed at each daa score. Near genesis several blocks share daa 0 (the
    /// DAA window lags height); everywhere this map is queried the chain is deep enough that daa is
    /// strictly increasing, and the assert below keeps it monotone.
    by_daa: std::collections::HashMap<u64, Hash64>,
}

impl Chain {
    async fn extend_plain(&mut self) -> (Hash64, Transaction) {
        self.extend_with(vec![], None).await
    }

    /// Append one linear block carrying `txs`. Returns (hash, coinbase).
    async fn extend_with(&mut self, txs: Vec<Transaction>, miner: Option<&MinerData>) -> (Hash64, Transaction) {
        self.counter += 1;
        let hash = hash_at(self.counter);
        let miner = miner.unwrap_or(&self.miner).clone();
        // Build through the REAL template with the txs included — the coinbase of a carrying block
        // must account for them (e.g. the DNS §E attestation reward rides the carrier's own coinbase).
        let mb = self.tc.build_utxo_valid_block_with_parents(hash, vec![self.tip], miner, txs);
        let coinbase = mb.transactions[0].clone();
        let hash = self.insert_expect_utxo_valid(mb).await;
        (hash, coinbase)
    }

    async fn insert_expect_utxo_valid(&mut self, mb: MutableBlock) -> Hash64 {
        let hash = mb.header.hash;
        let daa = mb.header.daa_score;
        let status = self
            .tc
            .validate_and_insert_block(mb.to_immutable())
            .virtual_state_task
            .await
            .unwrap_or_else(|e| panic!("harness block at daa {daa} rejected: {e:?}"));
        assert_eq!(status, BlockStatus::StatusUTXOValid, "linear harness block at daa {daa} must be UTXO-valid");
        assert!(daa >= self.tip_daa, "the linear harness chain must have monotone daa (got {daa} after {})", self.tip_daa);
        self.by_daa.entry(daa).or_insert(hash);
        self.tip = hash;
        self.tip_daa = daa;
        hash
    }
}

/// Resolve the clause-5/clause-9 draw for `leaf` minted as the child of `sp` (target DAA
/// `sp_daa + 1`), the way a real miner would: lagged DNS anchor + template-stamped beacon seed.
#[allow(clippy::too_many_arguments)]
fn draw_at(
    tc: &TestConsensus,
    sp: Hash64,
    target_interval: u64,
    leaf: &PalwPublicLeafV1,
    nullifier_preimage: Hash64,
    cert_hash: Hash64,
    prov_a: &ScriptPublicKey,
    prov_b: &ScriptPublicKey,
    miner: &MinerData,
) -> (bool, PalwAlgo4Facts) {
    let params = tc.params();
    let net_id = params.net.suffix().unwrap_or(0);
    let replica_bits = params.palw_lane_difficulty.genesis_replica_bits;
    let dns_params = params.dns_params.clone().unwrap();
    let anchor = resolve_palw_lagged_anchor(&tc.storage.headers_store, tc.reachability_service(), &dns_params, sp)
        .expect("a finality-buried DNS anchor must exist this deep into the chain");
    let anchor_header = tc.storage.headers_store.get_header(anchor.anchor_hash).unwrap();
    let anchor_facts = BeaconDnsAnchor {
        hash: anchor.anchor_hash,
        blue_score: anchor.anchor_blue_score,
        daa_score: anchor.anchor_daa_score,
        overlay_root: anchor_header.overlay_commitment_root,
    };
    let eligibility_beacon = anchor_header.palw_beacon_seed;
    let expected_chain_commit =
        chain_commit(&anchor_facts.hash, &dns_finality_certificate_hash_v1(&anchor_facts), target_interval, net_id);
    let digest = eligibility_hash(
        net_id,
        &eligibility_beacon,
        &expected_chain_commit,
        target_interval,
        &leaf.batch_id,
        leaf.leaf_index,
        &leaf.leaf_hash(),
        &nullifier_preimage,
    );
    let nb = nullifier_preimage.as_byte_slice();
    let nonce = u64::from_le_bytes([nb[0], nb[1], nb[2], nb[3], nb[4], nb[5], nb[6], nb[7]]);
    let win = palw_eligibility_win(&digest, replica_bits, nonce, &nullifier_preimage);
    let facts = PalwAlgo4Facts {
        sp,
        replica_bits,
        batch_id: leaf.batch_id,
        leaf_index: leaf.leaf_index,
        proof_type: leaf.proof_type,
        nullifier: nullifier_preimage,
        nonce,
        cert_hash,
        target_interval,
        expected_chain_commit,
        prov_a: prov_a.clone(),
        prov_b: prov_b.clone(),
        miner: miner.clone(),
        authority_seed: PALW_TEST_AUTHORITY_SEED,
    };
    (win, facts)
}

/// Extend the linear chain until the clause-9 lottery first selects `leaf` at a height ≥ `floor_daa`,
/// then mint the algo-4 block THERE as the next chain block. The leaf is FROZEN (registered on chain),
/// so this is exactly a miner's life: wait for an eligible height, then publish.
#[allow(clippy::too_many_arguments)]
async fn mint_first_win(
    chain: &mut Chain,
    beacon: &mut BeaconDriver,
    net_id: u32,
    storage_mass_parameter: u64,
    leaf: &PalwPublicLeafV1,
    nullifier_preimage: Hash64,
    cert_hash: Hash64,
    prov_a: &ScriptPublicKey,
    prov_b: &ScriptPublicKey,
    miner: &MinerData,
    seed_byte: u8,
    floor_daa: u64,
) -> (Hash64, PalwAlgo4Facts) {
    while chain.tip_daa + 1 < floor_daa {
        beacon.tick(chain, net_id, storage_mass_parameter).await;
        chain.extend_plain().await;
    }
    loop {
        beacon.tick(chain, net_id, storage_mass_parameter).await;
        let (win, facts) =
            draw_at(&chain.tc, chain.tip, chain.tip_daa + 1, leaf, nullifier_preimage, cert_hash, prov_a, prov_b, miner);
        if !win {
            chain.extend_plain().await;
            continue;
        }
        assert!(facts.target_interval < EXPIRY_EPOCH * EPOCH_LEN, "the winning height must stay inside the cert-active window");
        let mb = mint_algo4(&chain.tc, &facts, seed_byte, 0, |_| {});
        let hash = chain.insert_expect_utxo_valid(mb).await;
        return (hash, facts);
    }
}

fn credited(cb: &Transaction, spk: &ScriptPublicKey) -> u64 {
    cb.outputs.iter().filter(|o| &o.script_public_key == spk).map(|o| o.value).sum()
}

/// ADR-0044 keystone. One long test on purpose: the lifecycle, the pass and every assert share one
/// chain, exactly like production — splitting it would either re-run the long build per assert or
/// fall back to hand-seeding, which this harness exists to forbid.
#[tokio::test]
async fn palw_full_lifecycle_prune_then_replay_e2e() {
    kaspa_core::log::try_init_logger("info");

    let config = ConfigBuilder::new(SIMNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.palw_activation_daa_score = 0;
            p.palw_algo4_accept = true; // behaviour tests presuppose acceptance; the shipped default is pinned elsewhere
            p.palw_epoch_length_daa = EPOCH_LEN;
            // ~27 epochs with no DNS beacon: keep the lane in DegradedGrace throughout.
            p.palw_beacon_grace_epochs = 1_000;
            p.palw_nullifier_retention_daa = RETENTION;
            // Admission windows: max_batch_life = 2*2+1+10 = 15 ⇒ walk bound 800 < pruning 900.
            // lead=2 keeps activation=4 inside admission's [min, min+lead] scheduling slack while the
            // audit epoch (3) still has a non-zero seed target (epoch 2, the beacon round below).
            p.palw_batch_admission.registration_lead_epochs = 2;
            p.palw_batch_admission.audit_window_epochs = 1;
            p.palw_batch_admission.active_window_epochs = 10;
            p.palw_batch_admission.min_provider_bond_sompi = BOND_SOMPI;
            p.palw_batch_admission.min_leaf_bond_sompi = 0;
            // A 3-of-3 auditor slate over the three non-provider bonds; stake quorum 2/3.
            p.palw_audit_committee_size = 3;
            p.palw_audit_sample_size = 2;
            p.palw_audit_quorum_num = 2;
            p.palw_audit_quorum_den = 3;
            p.coinbase_maturity = 2;
            // Algo-3 v3 supporting blocks + max-easy replica lane, single-lane HOLD (the env shape).
            p.pow_blake2b_sha3_activation = ForkActivation::always();
            p.palw_lane_difficulty = LaneDifficultyParams {
                genesis_hash_bits: 0x207fffff,
                genesis_replica_bits: 0x207fffff,
                min_samples: 100_000,
                ..LaneDifficultyParams::INERT
            };
            p.min_difficulty_window_size = p.difficulty_window_size;
            // DNS: the `dns_v3_validator_drives_confirmed_anchor` recipe — GENESIS_ACTIVE
            // TwoDimensionalDominance params with trivial confirmation thresholds, so ONE bonded
            // validator + one attested epoch CONFIRMS the anchor. A confirmed anchor is what turns
            // the derived beacon seed non-zero (all-zero seeds fail the audit-seed resolver closed).
            {
                use kaspa_consensus_core::dns_finality::{STAKE_SCORE_SCALE, StakeScore};
                let mut dns = kaspa_consensus_core::config::params::DEVNET_PARAMS.dns_params.clone().unwrap();
                dns.dns_activation_daa_score = 0;
                dns.pos_v2_activation_daa_score = 0;
                dns.epoch_length_blocks = 2;
                dns.reward_uniqueness_window_blocks = 50;
                dns.max_reorg_horizon_blocks = 2;
                dns.attestation_epoch_length_blue_score = 3;
                dns.attestation_lag_blue_score = 2;
                dns.attestation_anchor_backoff_blue_score = 1;
                dns.stake_score_window_blue_score = 10_000;
                dns.required_work_depth = kaspa_consensus_core::BlueWorkType::ZERO;
                dns.required_stake_depth = StakeScore(STAKE_SCORE_SCALE / 2);
                // Health shrink: one validator cannot attest every 3-blue epoch of a 1,400-block
                // chain; zero quality/censorship floors keep DnsHealth::Active so the beacon seed
                // hash-chain runs (health thresholds have their own dedicated e2es).
                dns.stake_event_quality_floor_bps = 0;
                dns.stake_censorship_floor_bps = 0;
                p.dns_params = Some(dns);
            }
            // The REAL pruning pass, shrunk: the first pruning point lands on the finality boundary.
            p.blockrate.finality_depth = FINALITY;
            p.blockrate.pruning_depth = PRUNING;
        })
        .build();
    let walk_bound = config.params.palw_batch_admission.paid_work_walk_bound_daa(EPOCH_LEN);
    assert!(walk_bound < PRUNING, "harness params must keep the preset-pinned walk<pruning relation (walk {walk_bound})");

    let tc = TestConsensus::new(&config);
    let handles = tc.init();
    let storage_mass_parameter = config.params.storage_mass_parameter;
    let net_id = config.params.net.suffix().unwrap_or(0);
    let miner = MinerData::new(p2pkh_mldsa87_spk(&[0x07; 64]), vec![]);
    let mut chain = Chain {
        tc,
        tip: config.params.genesis.hash,
        tip_daa: 0,
        counter: 0,
        miner: miner.clone(),
        by_daa: std::collections::HashMap::from([(0u64, config.params.genesis.hash)]),
    };

    // ================= Funding: 9 coinbases paying dedicated keys =================
    // seeds[0..2] providers A/B, [2..5] auditors, [5..9] carrier funders (manifest/chunk/cert/spare).
    // A block's coinbase pays the miners of the blocks it MERGES, so the coinbase paying seed i's
    // key rides in block i+1: mine one block per seed, then harvest from each SUCCESSOR's coinbase.
    // seeds[0..2] providers A/B, [2..5] auditors, [5..8] carrier funders (manifest/chunk/cert),
    // [8] the DNS stake bond, [9] the DNS attestation shard, [10] spare.
    let seeds: [[u8; 32]; 11] = std::array::from_fn(|i| [0x51 + i as u8; 32]);
    let spks: Vec<ScriptPublicKey> =
        seeds.iter().map(|s| plain_p2pkh_spk(mldsa::generate_key_pair(*s).verification_key.as_ref())).collect();
    let mut funding: Vec<(TransactionOutpoint, u64, u64)> = Vec::new();
    let mut prev: Option<usize> = None;
    for i in 0..=seeds.len() {
        let fm = (i < seeds.len()).then(|| MinerData::new(spks[i].clone(), vec![]));
        let (_, cb) = chain.extend_with(vec![], fm.as_ref()).await;
        if let Some(pi) = prev {
            let (idx, out) =
                cb.outputs.iter().enumerate().find(|(_, o)| o.script_public_key == spks[pi]).expect("coinbase pays the key");
            funding.push((TransactionOutpoint::new(cb.id(), idx as u32), out.value, chain.tip_daa));
            assert!(out.value > BOND_SOMPI + CARRIER_FEE, "one coinbase must cover a bond plus fee");
        }
        prev = (i < seeds.len()).then_some(i);
    }
    assert_eq!(funding.len(), seeds.len());
    for _ in 0..3 {
        chain.extend_plain().await; // mature the coinbases (maturity 2)
    }

    // ================= Provider + auditor bonds through ACCEPTANCE (0x30) =================
    let bond_txs: Vec<Transaction> = (0..5)
        .map(|i| funded_bond_tx(seeds[i], funding[i], Hash64::from_bytes([0x60 + i as u8; 64]), storage_mass_parameter))
        .collect();
    let bond_outpoints: Vec<TransactionOutpoint> = bond_txs.iter().map(|tx| TransactionOutpoint::new(tx.id(), 0)).collect();
    // One bond per block: each bond tx carries ~169k storage mass against the 500k per-block cap.
    for tx in bond_txs {
        chain.extend_with(vec![tx], None).await;
    }
    chain.extend_plain().await; // the chain block that ACCEPTS the last bond carrier
    {
        let bonds = chain.tc.storage.palw_provider_bonds_store.read();
        for op in &bond_outpoints {
            assert!(bonds.get(op).is_ok(), "accepted bond {op} must be in the registry (prefix 241)");
        }
    }

    // ================= The batch: 3 leaves, manifest first (content-addressed), then the chunk =================
    // Fixed nullifier preimages: eligibility is a per-height ~50% lottery at these bits, so the mint
    // stage below finds a winning HEIGHT for the frozen leaf instead of grinding the leaf itself.
    let cand: [Hash64; 3] = [Hash64::from_bytes([0xC1; 64]), Hash64::from_bytes([0xC2; 64]), Hash64::from_bytes([0xC3; 64])];
    // REAL per-leaf DA objects: the DA obligations registered at chunk acceptance go `Satisfied`
    // only through a challenge→response round whose chunk proof must open the leaf's declared
    // `receipt_da_root` — so the root/len/chunk_count must commit to bytes we can actually serve.
    let da_bytes: Vec<Vec<u8>> = (0..3u8).map(|i| vec![0xB0 + i; 96]).collect();
    let da_commitments: Vec<kaspa_consensus_core::palw::da::PalwReceiptDaCommitmentV1> = da_bytes
        .iter()
        .map(|b| kaspa_consensus_core::palw::da::palw_receipt_da_commitment(1, b).expect("da commitment"))
        .collect();
    let prov_a_pk = mldsa::generate_key_pair(seeds[0]).verification_key.as_ref().to_vec();
    let prov_b_pk = mldsa::generate_key_pair(seeds[1]).verification_key.as_ref().to_vec();
    let prov_a = provider_bond_lock_spk(&prov_a_pk);
    let prov_b = provider_bond_lock_spk(&prov_b_pk);
    let shared_job = Hash64::from_bytes([0x09; 64]); // L0 and L2 share it: the G16 dup-work pair
    let make_leaf = |leaf_index: u32, commit: Hash64, job: Hash64, da: &kaspa_consensus_core::palw::da::PalwReceiptDaCommitmentV1| PalwPublicLeafV1 {
        version: 1,
        batch_id: Hash64::default(), // projected now; populated once the manifest fixes batch_id
        leaf_index,
        job_nullifier: job,
        ticket_nullifier_commitment: commit,
        model_profile_id: Hash64::from_bytes([0x01; 64]),
        runtime_class_id: Hash64::from_bytes([0x02; 64]),
        shape_id: 1,
        quantum_count: 1,
        proof_type: 1,
        provider_a_bond: bond_outpoints[0],
        provider_b_bond: bond_outpoints[1],
        provider_a_reward_script: prov_a.clone(),
        provider_b_reward_script: prov_b.clone(),
        ticket_authority_pk_hash: palw_authority_pk_hash(PALW_TEST_AUTHORITY_SEED),
        private_match_commitment: Hash64::default(),
        receipt_da_object_version: da.object_version,
        receipt_da_root: da.root,
        receipt_da_object_len: da.object_len,
        receipt_da_chunk_count: da.chunk_count,
        receipt_v3_compute_set_id: Hash64::default(),
        receipt_v3_job_challenge: Hash64::default(),
        receipt_v3_issued_epoch: 0,
        receipt_v3_expires_epoch: 0,
        registered_epoch: 0,
        activation_epoch: ACTIVATION_EPOCH,
        expiry_epoch: EXPIRY_EPOCH,
        leaf_bond_sompi: 0,
    };
    let projected: Vec<PalwPublicLeafV1> = vec![
        make_leaf(0, ticket_nullifier_commitment(&cand[0]), shared_job, &da_commitments[0]),
        make_leaf(1, ticket_nullifier_commitment(&cand[1]), Hash64::from_bytes([0x0A; 64]), &da_commitments[1]),
        make_leaf(2, ticket_nullifier_commitment(&cand[2]), shared_job, &da_commitments[2]),
    ];
    let projected_hashes: Vec<Hash64> = projected.iter().map(|l| l.leaf_hash()).collect();
    // The root is over the batch_id-ZEROED projections (`batch_id == content_id()` contains the root,
    // so a populated-leaf root would be self-referential). Same derivation the acceptance gate uses.
    let leaf_root = palw_leaf_merkle_root(&projected_hashes);
    let mut manifest = PalwBatchManifestV1 {
        version: 1,
        batch_id: Hash64::default(),
        registration_epoch: 0,
        model_profile_id: Hash64::from_bytes([0x01; 64]),
        runtime_class_id: Hash64::from_bytes([0x02; 64]),
        leaf_count: 3,
        chunk_count: 1,
        leaf_root,
        descriptor_root: Hash64::default(),
        total_leaf_bond_sompi: 0,
        audit_policy_id: Hash64::default(),
        activation_not_before_epoch: ACTIVATION_EPOCH,
        expiry_epoch: EXPIRY_EPOCH,
    };
    manifest.batch_id = manifest.content_id();
    let batch_id = manifest.batch_id;
    let leaves: Vec<PalwPublicLeafV1> = projected
        .iter()
        .map(|l| {
            let mut leaf = l.clone();
            leaf.batch_id = batch_id;
            leaf
        })
        .collect();
    let proofs = (0..3).map(|i| palw_leaf_merkle_proof(&projected_hashes, i).expect("membership proof")).collect::<Vec<_>>();
    let chunk = PalwLeafChunkV1 { version: PALW_LEAF_CHUNK_VERSION_V2, batch_id, chunk_index: 0, leaves: leaves.clone(), proofs };

    // Manifest (epoch 0: registration_epoch is pinned to the ACCEPT epoch) then the chunk.
    assert!(chain.tip_daa + 2 < EPOCH_LEN, "manifest must be accepted inside epoch 0");
    let manifest_tx = funded_overlay_tx(
        seeds[5],
        funding[5],
        SUBNETWORK_ID_PALW_BATCH_MANIFEST,
        borsh::to_vec(&manifest).unwrap(),
        storage_mass_parameter,
    );
    chain.extend_with(vec![manifest_tx], None).await;
    chain.extend_plain().await;
    assert!(chain.tc.storage.palw_store.batch_manifest(batch_id).is_ok(), "accepted manifest must be in the blob store");

    // ================= DNS beacon vertical: stake bond + one canonical-anchor attestation =================
    // Without a CONFIRMED DNS anchor every derived beacon seed stays zero and
    // `resolve_palw_audit_epoch_seed` fails closed — the attested certificate would be impossible.
    // One bond + one shard早期 confirm the anchor; from the next PALW epoch boundary the seed hash-chain
    // is non-zero forever (degraded carry keeps it non-zero even with no further attestations).
    let dns_v = dns_harness::harness_validator(seeds[8]);
    let (dns_bond_tx, _vid, _reward) = dns_harness::funded_signed_bond_tx(
        seeds[8],
        funding[8].0,
        funding[8].1,
        funding[8].2,
        funding[8].1 - CARRIER_FEE,
        0,
        storage_mass_parameter,
    );
    let dns_bond_outpoint = TransactionOutpoint::new(dns_bond_tx.id(), 0);
    chain.extend_with(vec![dns_bond_tx], None).await;
    for _ in 0..8 {
        chain.extend_plain().await; // bury blue-score epochs so a ready, bond-active canonical anchor exists
    }
    {
        use kaspa_consensus_core::dns_finality::ready_epoch_from_tip_blue_score;
        let dns = config.params.dns_params.clone().unwrap();
        let sink = chain.tc.get_sink();
        let vp = chain.tc.virtual_processor();
        let sink_blue = chain.tc.storage.headers_store.get_blue_score(sink).unwrap();
        let lr = ready_epoch_from_tip_blue_score(sink_blue, dns.attestation_epoch_length_blue_score, dns.attestation_lag_blue_score)
            .expect("a DNS attestation epoch is ready");
        let anchor = vp.canonical_anchor_by_blue_score(lr, sink, &dns).expect("canonical anchor for the ready epoch");
        let att = dns_harness::build_signed_attestation(
            &dns_v,
            config.params.genesis.hash.as_byte_slice(),
            dns_bond_outpoint,
            anchor.epoch,
            anchor.anchor_hash,
            anchor.anchor_daa_score,
            Hash64::default(),
        );
        let shard_tx =
            dns_harness::funded_signed_shard_tx(seeds[9], funding[9].0, funding[9].1, funding[9].2, att, storage_mass_parameter);
        chain.extend_with(vec![shard_tx], None).await;
        chain.extend_plain().await;
    }

    // ================= Beacon driver: continuous commit/reveal cadence =================
    // Instantiated in epoch 0 so the first commit targets epoch 2; ticked at every advance through
    // the whole mint window (max target = the batch expiry epoch), keeping the lagged activation
    // window Healthy so `Certified → Active` stays open for both the below-pp and above-pp mints.
    let mut beacon = BeaconDriver {
        bond: dns_bond_outpoint,
        seed: seeds[8],
        wallet_seed: seeds[10],
        wallet: funding[10],
        committed_target: 0,
        revealed_target: 0,
        max_target: EXPIRY_EPOCH,
    };
    beacon.tick(&mut chain, net_id, storage_mass_parameter).await;






    // The DA obligation registration for an accepted leaf anchors at a beacon buried by the FIXED
    // policy (PalwDaPolicyV1::STRICT_TESTNET, min_beacon_burial_daa = 100): a chunk accepted any
    // earlier fail-stops the node. Registration stays pinned to epoch 0 via the manifest; only the
    // chunk carrier waits.
    while chain.tip_daa + 1 < 2 * EPOCH_LEN + 5 {
        beacon.tick(&mut chain, net_id, storage_mass_parameter).await;
        chain.extend_plain().await;
    }
    let chunk_tx = funded_overlay_tx(
        seeds[6],
        funding[6],
        SUBNETWORK_ID_PALW_LEAF_CHUNK,
        borsh::to_vec(&chunk).unwrap(),
        storage_mass_parameter,
    );
    chain.extend_with(vec![chunk_tx], None).await;
    chain.extend_plain().await;
    for i in 0..3u32 {
        assert!(chain.tc.storage.palw_store.leaf(batch_id, i).is_ok(), "accepted leaf {i} must be in the blob store");
    }

    // ================= DA challenge → response: drive every obligation to Satisfied =================
    // `certificate_allowed(batch)` demands EVERY registered obligation `Satisfied`, and the only
    // satisfaction path is a challenger-bond challenge answered by the provider with a real chunk
    // proof (the live devnet's CertAbsent root cause, exercised here on purpose). 3 leaves × 2
    // providers × samples_per_provider(1) = 6 obligations; the auditors take turns as challengers
    // (max_challenges_per_bond_per_epoch = 4).
    {
        use kaspa_consensus_core::palw::da::{
            PALW_DA_CHALLENGE_V1_MLDSA87_CONTEXT, PALW_DA_CHALLENGE_VERSION_V1, PALW_DA_RESPONSE_V1_MLDSA87_CONTEXT,
            PALW_DA_RESPONSE_VERSION_V1, PalwDaChallengeV1, PalwDaResponseV1, palw_receipt_da_chunk_proof,
        };
        let policy_response_window: u64 = 200; // PalwDaPolicyV1::STRICT_TESTNET.response_window_daa
        let vp = chain.tc.virtual_processor();
        let da_state = vp.palw_da_parent_state(chain.tip, chain.tip_daa).0;
        let mut obligations: Vec<_> =
            da_state.obligations.values().filter(|o| o.batch_id == batch_id).cloned().collect();
        obligations.sort_by_key(|o| o.obligation_id);
        assert_eq!(obligations.len(), 6, "3 leaves x 2 providers x 1 sample = 6 registered obligations");
        // One funding wallet chained through change outputs carries all 12 DA txs.
        let mut wallet = (funding[7].0, funding[7].1, funding[7].2);
        for (k, ob) in obligations.iter().enumerate() {
            beacon.tick(&mut chain, net_id, storage_mass_parameter).await;
            let challenger_i = 2 + (k % 3); // auditors only — a provider may not challenge itself
            let ch_kp = mldsa::generate_key_pair(seeds[challenger_i]);
            let opened = chain.tip_daa + 2; // the carrier lands at tip+1, its ACCEPTING chain block at tip+2
            let mut challenge = PalwDaChallengeV1 {
                version: PALW_DA_CHALLENGE_VERSION_V1,
                network_id: net_id,
                obligation_id: ob.obligation_id,
                challenge_epoch: opened / EPOCH_LEN,
                opened_daa_score: opened,
                response_deadline_daa_score: (opened + policy_response_window).min(ob.retention_until_daa_score),
                challenger_bond: bond_outpoints[challenger_i],
                challenger_owner_public_key: ch_kp.verification_key.as_ref().to_vec(),
                challenge_nonce: Hash64::from_bytes([0xE0 + k as u8; 64]),
                signature: vec![],
            };
            let cd = challenge.signing_hash();
            challenge.signature =
                mldsa::sign(&ch_kp.signing_key, cd.as_bytes().as_slice(), PALW_DA_CHALLENGE_V1_MLDSA87_CONTEXT, [0x21; 32])
                    .expect("sign challenge")
                    .as_ref()
                    .to_vec();
            let challenge_id = challenge.challenge_id();
            let ch_tx = funded_overlay_tx(
                seeds[7],
                wallet,
                kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_DA_CHALLENGE,
                borsh::to_vec(&challenge).unwrap(),
                storage_mass_parameter,
            );
            wallet = (TransactionOutpoint::new(ch_tx.id(), 0), wallet.1 - CARRIER_FEE, chain.tip_daa + 1);
            chain.extend_with(vec![ch_tx], None).await;
            chain.extend_plain().await; // the accepting chain block (opened_daa == its daa)

            let leaf_i = ob.leaf_index as usize;
            let provider_i = if ob.provider_bond == bond_outpoints[0] { 0 } else { 1 };
            assert_eq!(ob.provider_bond, bond_outpoints[provider_i], "obligation names one of the leaf's two provider bonds");
            let pr_kp = mldsa::generate_key_pair(seeds[provider_i]);
            let mut response = PalwDaResponseV1 {
                version: PALW_DA_RESPONSE_VERSION_V1,
                network_id: net_id,
                challenge_id,
                provider_bond: ob.provider_bond,
                provider_owner_public_key: pr_kp.verification_key.as_ref().to_vec(),
                chunk_proof: palw_receipt_da_chunk_proof(1, &da_bytes[leaf_i], ob.chunk_index).expect("chunk proof"),
                signature: vec![],
            };
            let rd = response.signing_hash();
            response.signature =
                mldsa::sign(&pr_kp.signing_key, rd.as_bytes().as_slice(), PALW_DA_RESPONSE_V1_MLDSA87_CONTEXT, [0x22; 32])
                    .expect("sign response")
                    .as_ref()
                    .to_vec();
            let rs_tx = funded_overlay_tx(
                seeds[7],
                wallet,
                kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_DA_RESPONSE,
                borsh::to_vec(&response).unwrap(),
                storage_mass_parameter,
            );
            wallet = (TransactionOutpoint::new(rs_tx.id(), 0), wallet.1 - CARRIER_FEE, chain.tip_daa + 1);
            chain.extend_with(vec![rs_tx], None).await;
            chain.extend_plain().await;
        }
        let da_after = chain.tc.virtual_processor().palw_da_parent_state(chain.tip, chain.tip_daa).0;
        assert!(da_after.certificate_allowed(&batch_id), "all six obligations must be Satisfied before the certificate");
        funding[7] = wallet; // the cert carrier keeps spending the same chained wallet
    }

    // ================= The attested certificate (audit epoch 3, carried in epoch 3) =================
    // `validate_certificate` pins audit <= certificate_epoch < activation < expiry, so the carrier
    // must sit inside epoch 3 exactly (activation is 4).
    while (chain.tip_daa + 1) / EPOCH_LEN < AUDIT_EPOCH {
        beacon.tick(&mut chain, net_id, storage_mass_parameter).await;
        chain.extend_plain().await;
    }
    let carrier_epoch = (chain.tip_daa + 1) / EPOCH_LEN;
    assert_eq!(carrier_epoch, AUDIT_EPOCH, "the cert carrier sits in the audit epoch");
    let audit_epoch: u64 = AUDIT_EPOCH;
    let pov_daa = audit_epoch * EPOCH_LEN;
    let prev_seed = resolve_palw_audit_epoch_seed(
        &chain.tc.storage.headers_store,
        chain.tc.reachability_service(),
        chain.tip,
        0,
        EPOCH_LEN,
        audit_epoch,
    )
    .expect("the audit-epoch seed R_0 must resolve on this chain");
    let bond_view = {
        let bonds = chain.tc.storage.palw_provider_bonds_store.read();
        ProviderBondView::from_records(bond_outpoints.iter().map(|op| (*op, (*bonds.get(op).expect("bond record")).clone())))
    };
    let (excluded_creds, excluded_groups) = {
        let bonds = chain.tc.storage.palw_provider_bonds_store.read();
        let mut creds = HashSet::new();
        let mut groups = HashSet::new();
        for op in [&bond_outpoints[0], &bond_outpoints[1]] {
            let rec = bonds.get(op).expect("provider bond record");
            creds.insert(rec.owner_pubkey_hash);
            groups.insert(rec.operator_group_id);
        }
        (creds, groups)
    };
    let (slate, auditor_set_commitment) =
        select_weighted_auditor_committee(&prev_seed, &batch_id, &bond_view, pov_daa, &excluded_creds, &excluded_groups, 3);
    assert_eq!(slate.len(), 3, "all three auditor bonds must be selected (providers are excluded)");
    let sampled = palw_deterministic_sample(&prev_seed, &batch_id, 3, 2);
    let sample_root = palw_audit_sample_root(&sampled.iter().map(|i| leaves[*i as usize].receipt_da_root).collect::<Vec<_>>());
    let approving_stake = slate.iter().fold(0u128, |t, m| t.saturating_add(m.weight));
    let votes: Vec<PalwAuditorVoteV2> = slate
        .iter()
        .map(|member| {
            let auditor_seed = (0..5)
                .find(|i| bond_outpoints[*i] == member.representative)
                .map(|i| seeds[i])
                .expect("slate member must be one of the harness bonds");
            let mut vote = PalwAuditorVoteV2 {
                bond_outpoint: member.representative,
                vote: 1,
                checked_leaf_bitmap_root: Hash64::default(),
                passed_leaf_count: 3,
                rejected_leaf_bitmap_root: Hash64::default(),
                signature: vec![],
            };
            let digest = vote.signing_hash(net_id, &batch_id, audit_epoch, &sample_root);
            let kp = mldsa::generate_key_pair(auditor_seed);
            vote.signature = mldsa::sign(&kp.signing_key, digest.as_bytes().as_slice(), PALW_AUDITOR_V2_MLDSA87_CONTEXT, [0x2a; 32])
                .expect("sign auditor vote")
                .as_ref()
                .to_vec();
            vote
        })
        .collect();
    let cert = PalwBatchCertificateV2 {
        version: PALW_BATCH_CERTIFICATE_VERSION_V2,
        batch_id,
        manifest_hash: manifest.content_id(),
        leaf_root,
        audit_beacon_epoch: audit_epoch,
        audit_sample_root: sample_root,
        passed_leaf_count: 3,
        rejected_leaf_bitmap_root: Hash64::default(),
        certificate_epoch: carrier_epoch,
        activation_epoch: ACTIVATION_EPOCH,
        expiry_epoch: EXPIRY_EPOCH,
        auditor_set_commitment,
        approving_stake,
        votes,
    };
    let cert_hash = cert.hash();

    // ---- G13 WITHHOLD (ADR-0045): a certificate whose committee majority withholds is REFUSED ----
    // Identical envelope, but only ONE of the three selected auditors votes: pass stake 1/3 < the 2/3
    // stake quorum over the FULL re-derived slate (withholding counts against quorum — the
    // denominator is the slate, not the voters). The acceptance walk must drop it silently: the blob
    // never reaches the store, and the batch stays un-Certified until the honest quorum lands below.
    {
        let mut withheld = cert.clone();
        withheld.votes.truncate(1);
        withheld.approving_stake = slate
            .iter()
            .find(|m| m.representative == withheld.votes[0].bond_outpoint)
            .map(|m| m.weight)
            .expect("the surviving vote's bond is in the slate");
        let withheld_hash = withheld.hash();
        assert_ne!(withheld_hash, cert_hash);
        let withheld_tx = funded_overlay_tx(
            seeds[7],
            funding[7],
            SUBNETWORK_ID_PALW_BATCH_CERT,
            borsh::to_vec(&withheld).unwrap(),
            storage_mass_parameter,
        );
        funding[7] = (TransactionOutpoint::new(withheld_tx.id(), 0), funding[7].1 - CARRIER_FEE, chain.tip_daa + 1);
        chain.extend_with(vec![withheld_tx], None).await;
        chain.extend_plain().await;
        assert!(
            chain.tc.storage.palw_store.certificate(withheld_hash).is_err(),
            "a quorum-less (withheld) certificate must NOT pass the acceptance attestation gate"
        );
    }

    // The honest, quorum-meeting certificate is accepted (store gate = verify_certificate_attestation).
    let cert_tx = funded_overlay_tx(
        seeds[7],
        funding[7],
        SUBNETWORK_ID_PALW_BATCH_CERT,
        borsh::to_vec(&cert).unwrap(),
        storage_mass_parameter,
    );
    chain.extend_with(vec![cert_tx], None).await;
    chain.extend_plain().await;
    assert!(
        chain.tc.storage.palw_store.certificate(cert_hash).is_ok(),
        "the ATTESTED certificate must pass verify_certificate_attestation on the acceptance path"
    );
    // NOTE on the reorg dimension of G13: the AUTHORITATIVE fork-local coordinate is the accepted
    // lifecycle / reward path, whose cross-fork behavior across a REAL sink reorg is already pinned
    // by `palw_algo4_sink_reorg_cross_fork_nullifier_replay_e2e`. The v3 body-stage view's `cert_hash`
    // is deliberately NON-authoritative (ADR-0040 CERT-TRUST: `apply_certificate` verifies nothing;
    // the store gate above is the bound, and a ticket resolves against the store), so it is not a
    // sound surface for a reorg assertion here — the withhold refusal above is the store-gated G13
    // point this harness adds.

    // ================= Mints (the clause-9 lottery over heights, leaves frozen) =================
    // Batch active from epoch 2 (daa 100): early mint W = leaf 0, then the G16 dup M2 = leaf 2.
    let w_floor = (chain.tip_daa + 2).max(ACTIVATION_EPOCH * EPOCH_LEN + 2);
    let (w_hash, w_facts) = mint_first_win(
        &mut chain, &mut beacon, net_id, storage_mass_parameter, &leaves[0], cand[0], cert_hash, &prov_a, &prov_b, &miner, 0xA0,
        w_floor,
    )
    .await;
    let w_daa = chain.tip_daa;
    let (_, a1_w_coinbase) = chain.extend_plain().await; // W's merger: pays the leaf's providers ONCE
    assert!(credited(&a1_w_coinbase, &prov_a) > 0, "W's merger must pay provider A (the ADR-0044 payment stage)");
    assert!(credited(&a1_w_coinbase, &prov_b) > 0, "W's merger must pay provider B");

    // G16 baseline: leaf 2 shares leaf 0's job_nullifier — its mint pays NOTHING (dup work within
    // the bounded selected-chain walk), pre-pass.
    let (_m2_hash, _m2_facts) = mint_first_win(
        &mut chain, &mut beacon, net_id, storage_mass_parameter, &leaves[2], cand[2], cert_hash, &prov_a, &prov_b, &miner, 0xA4,
        w_daa + 2,
    )
    .await;
    let (_, a1_m2_coinbase) = chain.extend_plain().await;
    assert_eq!(credited(&a1_m2_coinbase, &prov_a), 0, "the dup-job mint pays provider A nothing (G16)");
    assert_eq!(credited(&a1_m2_coinbase, &prov_b), 0, "the dup-job mint pays provider B nothing (G16)");

    // Late mint M1 = leaf 1, minted at ≈ daa 950 — ABOVE the first pruning point (a finality
    // sample at 400 or 800) yet still inside the batch's active window (expiry ≈ daa 1500).
    let (m1_hash, m1_facts) = mint_first_win(
        &mut chain, &mut beacon, net_id, storage_mass_parameter, &leaves[1], cand[1], cert_hash, &prov_a, &prov_b, &miner, 0xA8,
        7 * EPOCH_LEN,
    )
    .await;
    let m1_daa = chain.tip_daa;
    // M1's merger needs a hash that beats the reuse sibling R1 in the reuse-merger's selected-parent
    // tiebreak: R1 sits at the SAME depth as M1 (child of M1's parent), so a1_m1 and R1 carry equal
    // (rounded) blue_work and GHOSTDAG breaks the tie by HASH. a1_m1 MUST win — window(a1_m1) is the
    // seed that carries M1's ticket (window(R1) does not: R1's past excludes M1), so an a1_m1 SP is
    // what recolors the reuse red. A near-max prefix wins deterministically over any computed R1 hash.
    // It does NOT stick the virtual sink: GHOSTDAG orders by blue_work FIRST, and the deep linear
    // blocks below outrank this shallow block by ACCUMULATED work regardless of hash (proven by the
    // sink tracking the tip). NOT all-0xFF (a DNS sink sentinel) and NOT all-0xFE (== ORIGIN); an 0xFF
    // prefix with a distinct tail wins the tiebreak and stays an ordinary id.
    let a1_m1_hash = {
        let mut b = [0xFF; 64];
        b[32..].fill(0xA1);
        Hash64::from_bytes(b)
    };
    let a1_m1_coinbase = {
        let mb = chain.tc.build_utxo_valid_block_with_parents(a1_m1_hash, vec![m1_hash], miner.clone(), vec![]);
        let cb = mb.transactions[0].clone();
        let daa = mb.header.daa_score;
        let status = chain.tc.validate_and_insert_block(mb.to_immutable()).virtual_state_task.await.unwrap();
        assert_eq!(status, BlockStatus::StatusUTXOValid, "M1's merger must be UTXO-valid");
        chain.by_daa.entry(daa).or_insert(a1_m1_hash);
        chain.tip = a1_m1_hash;
        chain.tip_daa = daa;
        cb
    };
    assert!(credited(&a1_m1_coinbase, &prov_a) > 0, "M1's merger must pay provider A");
    assert!(credited(&a1_m1_coinbase, &prov_b) > 0, "M1's merger must pay provider B");

    // ---- (2a) PRE-PASS recolor baseline: a same-height reuse of M1's ticket is recolored RED ----
    // R1 reuses M1's exact winning ticket at M1's own height; P_R merges {M1's merger, R1}. The merge
    // recolors R1 red, driven by the persisted nullifier window window(a1_m1) carries (window(R1)
    // does NOT — R1's past excludes M1). This is the credit-denial mechanism the ADR requires; it is
    // demonstrated here on real minted tickets. NOTE: with the preset-pinned `walk_bound <
    // pruning_depth`, the whole batch window (and hence this reuse-merger) is BELOW the first pruning
    // point (which tracks `sink − pruning_depth`), so P_R is itself pruned by the pass. The post-pass
    // survivor of the reuse-catching capability is the FRONTIER's retention window (assertion 2b), the
    // exact object a joining node re-imports; the recolor code path over that window is the same one
    // the pre-pruning nullifier e2es (`palw_algo4_*_nullifier_*`) pin.
    let (win_again, r1_facts) = draw_at(
        &chain.tc,
        m1_facts.sp,
        m1_facts.target_interval,
        &leaves[1],
        cand[1],
        cert_hash,
        &prov_a,
        &prov_b,
        &miner,
    );
    assert!(win_again, "the clause-9 draw is deterministic: the same (leaf, height) must still win");
    let r1 = mint_algo4(&chain.tc, &r1_facts, 0xB0, 2, |_| {});
    let r1_hash = r1.header.hash;
    let r1_status = chain.tc.validate_and_insert_block(r1.to_immutable()).virtual_state_task.await.unwrap();
    assert!(
        matches!(r1_status, BlockStatus::StatusUTXOValid | BlockStatus::StatusUTXOPendingVerification),
        "the reuse block is body-valid at M1's height — got {r1_status:?}"
    );
    // P_R (the reuse-merger) keeps a LOW hash so it does NOT attract the virtual sink: it is a chain
    // tip that the deep linear blocks below must overtake, and while blue_work ties they would tie on
    // hash — a high-hash p_r would sit at the front of that tie and stall sink advance. A normal
    // hash_at keeps p_r an ordinary tip the linear chain immediately passes.
    chain.counter += 1;
    let p_r_hash = hash_at(chain.counter);
    let p_r = chain.tc.build_utxo_valid_block_with_parents(p_r_hash, vec![a1_m1_hash, r1_hash], miner.clone(), vec![]);
    let p_r_daa = p_r.header.daa_score;
    assert_eq!(
        chain.tc.validate_and_insert_block(p_r.to_immutable()).virtual_state_task.await.unwrap(),
        BlockStatus::StatusUTXOValid,
        "the reuse-merger is accepted into the DAG"
    );
    chain.by_daa.entry(p_r_daa).or_insert(p_r_hash);
    chain.tip = p_r_hash;
    chain.tip_daa = p_r_daa;
    assert_eq!(
        chain.tc.ghostdag_store().get_selected_parent(p_r_hash).unwrap(),
        a1_m1_hash,
        "the merger's selected parent is M1's merger"
    );
    assert!(
        chain.tc.ghostdag_store().get_mergeset_reds(p_r_hash).unwrap().contains(&r1_hash),
        "the same-height nullifier reuse is recolored RED by the persisted window (pre-pass baseline)"
    );

    // ================= The long build + the REAL pruning pass =================
    // EMPIRICAL (measured on this fixture): the pruning point tracks `sink − pruning_depth`, and its
    // first advance from genesis jumps straight to ~0.9·pruning_depth. With the preset-pinned
    // `walk_bound < pruning_depth`, the ENTIRE batch window (daa 200-500) — and therefore W, M1 and
    // the reuse-merger P_R — is BELOW that first pruning point: no mint can ever be above it. So the
    // pass prunes the whole minted region; the surviving reuse-catching object is the FRONTIER's
    // retention window (assertion 2b). The pruning worker is async, so grow while yielding to it and
    // stop at the first non-genesis pp; the yields keep it from lagging and overshooting.
    let genesis_hash = config.params.genesis.hash;
    let mut grown = 0u64;
    while chain.tc.pruning_point() == genesis_hash {
        chain.extend_plain().await;
        grown += 1;
        if grown % 20 == 0 {
            tokio::time::sleep(Duration::from_millis(20)).await; // let the async pruning worker keep pace
        }
        assert!(grown < 4000, "the pruning point must advance before the guard length");
    }
    let pp = chain.tc.pruning_point();
    let pp_daa = chain.tc.storage.headers_store.get_header(pp).unwrap().daa_score;
    assert!(m1_daa < pp_daa, "layout: every mint (W {w_daa}, M1 {m1_daa}, P_R {p_r_daa}) must fall BELOW the pruning point {pp_daa}");
    assert!(pp_daa < w_daa + RETENTION, "layout: W must still be inside the retention window AT the pp (carried by the frontier)");
    assert!(pp_daa < w_daa + walk_bound, "layout: W's paid-work must sit inside the walk bound below the pp");
    // Wait for the async deletion pass to remove the pruned region's per-block rows.
    let mut waited = 0u64;
    while chain.tc.storage.palw_nullifier_store.get(w_hash).is_ok() {
        assert!(waited < 600, "the pruning pass must delete pruned-region nullifier rows within the timeout");
        tokio::time::sleep(Duration::from_millis(100)).await;
        waited += 1;
    }

    // ---- (1) store audit: the pruned region's per-block nullifier rows are gone; above-pp rows survive ----
    // W (the earliest mint, deepest below the pp) is unambiguously in the pruned past. M1 / P_R sit
    // closer to the pp and may fall inside the consensus retention buffer kept just below the
    // boundary, so the store-deletion assertion targets W.
    assert!(chain.tc.storage.palw_nullifier_store.get(w_hash).is_err(), "W's per-block nullifier row must be pruned");
    let above = *chain.by_daa.get(&(pp_daa + 1)).expect("a chain block at pp_daa+1 must exist on the linear chain");
    let above_set = chain.tc.storage.palw_nullifier_store.get(above).expect("rows above the pp must survive the pass");
    assert!(
        above_set.contains(&w_facts.nullifier) && above_set.contains(&m1_facts.nullifier),
        "the window folded through the pruned region (still within retention) carries both minted tickets"
    );

    // ---- (2b) prune-then-replay survivor: the FRONTIER carries the retention window across the pass ----
    // Every minted block is pruned, so the reuse-catching capability is no longer a per-block row — it
    // is the pruned frontier a joining node re-imports. It must equal the pp's own persisted window and
    // carry BOTH minted nullifiers (both inside retention of the pp), so a re-import still recolors a
    // reuse of either ticket red — the exact post-pruning form of the pre-pass baseline (2a).
    let snapshot = chain
        .tc
        .pruning_point_palw_snapshot()
        .expect("the pass must persist a context-valid pruning snapshot for the ACCEPTANCE-BUILT lifecycle");
    assert_eq!(snapshot.payload.pruning_point, pp);
    let pp_set = chain.tc.storage.palw_nullifier_store.get(pp).expect("the pruning point's own row survives");
    assert_eq!(
        snapshot.payload.frontier.active_nullifiers, *pp_set,
        "PalwPrunedFrontierV1.active_nullifiers must be exactly the pruning point's persisted window"
    );
    assert!(
        snapshot.payload.frontier.active_nullifiers.contains(&w_facts.nullifier)
            && snapshot.payload.frontier.active_nullifiers.contains(&m1_facts.nullifier),
        "the frontier carries both minted tickets (inside retention), so a re-import still catches a reuse"
    );

    // ---- (3) a replay at a PRUNED height is structurally impossible (clause-5 coupling) ----
    // W's ticket is bound by clause 5 to `target_daa_interval == W's own DAA`, which is now buried
    // BELOW the pruning point. The per-block nullifier windows across that whole region are gone
    // (assertion 1), and so are W's selected-parent's — so a windowed same-height replay cannot be
    // reconstructed on this node at all. (Attempting to INSERT such a block does not fail gracefully:
    // GHOSTDAG seeds from the pruned SP's window and fail-closes — the node's defense, not a path a
    // test can drive. The structural fact is what the ADR asserts.) A replay's only re-entry is the
    // frontier import, which carries the nullifier within retention (assertion 1's frontier check).
    assert!(w_facts.target_interval <= pp_daa, "W's clause-5 target interval is buried at/below the pruning point");
    assert!(
        chain.tc.storage.palw_nullifier_store.get(w_facts.sp).is_err(),
        "W's selected parent's window is pruned, so the same-height replay's seed no longer exists locally"
    );

    // ---- (4) window-exit is SPEC: beyond retention the ticket ranks as fresh ----
    while chain.tip_daa < m1_daa + RETENTION {
        chain.extend_plain().await;
    }
    let tip_set = chain.tc.storage.palw_nullifier_store.get(chain.tip).unwrap();
    assert!(!tip_set.contains(&w_facts.nullifier), "W's ticket left every live window (retention)");
    assert!(!tip_set.contains(&m1_facts.nullifier), "M1's ticket left every live window (retention)");
    let mut fresh = (*tip_set).clone();
    assert!(fresh.insert(m1_facts.nullifier, chain.tip_daa), "out-of-window reuse ranks as a FRESH ticket — by design (ADR-0044)");

    // ---- (5)+(6) the pruning snapshot payload: G16 carry across the pass, and ADR-0042 §1c ----
    let payload_paid = palw_pruning_payload_paid_work_nullifiers(&snapshot.payload, walk_bound);
    assert!(
        payload_paid.contains(&shared_job),
        "G16 across the pass: the below-boundary paid-work row (W's job) must be carried by the snapshot"
    );
    let vp = chain.tc.virtual_processor();
    let store_paid = vp.palw_paid_work_window(pp, pp_daa);
    assert_eq!(
        payload_paid.iter().copied().collect::<HashSet<_>>(),
        store_paid,
        "ADR-0042 1c: payload-derived paid-work nullifiers == live store derivation at the pp"
    );
    let payload_da_root = palw_pruning_payload_da_state_root(&snapshot.payload);
    let store_da_root = vp.palw_da_parent_state(pp, pp_daa).0.state_root();
    assert_eq!(payload_da_root, store_da_root, "ADR-0042 1c: payload-derived DA state root == live store derivation at the pp");

    chain.tc.shutdown(handles);
}
