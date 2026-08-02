//! ADR-0039 P0 — **devnet-palw ONLY** in-node "mint one algo-4 proof-of-LLM block" path for a LIVE
//! running kaspad. This is the running-daemon twin of the in-process E2E
//! (`palw_algo4_devnet_palw_preset_e2e`): it seeds a **mock-k=2** leaf / certificate / Active-view into the
//! real consensus stores off the current sink, then mints an algo-4 (replica-lane) block off the sink via
//! the real template builder and returns it ready to submit through the normal pipeline. Consensus treats
//! the inference leaf as **opaque** (never re-runs the model), so a mock leaf is indistinguishable to the
//! validator; the `DegradedGrace` beacon path accepts the block with no live DNS quorum. NOT real value
//! (throwaway devnet, seeded stores, mock inference). The caller submits the returned block and awaits its
//! `validate_and_insert_block` future.

use std::sync::Arc;

use kaspa_consensus_core::{
    api::ConsensusApi,
    block::{Block, TemplateBuildMode, TemplateTransactionSelector},
    coinbase::MinerData,
    config::params::DEVNET_PALW_PARAMS,
    dns_finality::p2pkh_mldsa87_spk,
    header::PalwHeaderFields,
    palw::{
        BeaconDnsAnchor, PALW_BATCH_CERTIFICATE_VERSION_V2, PalwBatchCertificateV2, PalwBatchLifecycleV1, PalwBatchStatus,
        PalwBatchViewV1, PalwPublicLeafV1, chain_commit, dns_finality_certificate_hash_v1, eligibility_hash, palw_eligibility_win,
        palw_leaf_merkle_root, ticket_nullifier_commitment,
    },
    pow_layer0::POW_ALGO_ID_PALW_REPLICA,
    tx::{Transaction, TransactionId, TransactionOutpoint},
};
use kaspa_hashes::Hash64;

use super::Consensus;
use crate::{
    model::stores::{headers::HeaderStoreReader, palw::PalwStore},
    processes::palw::resolve_palw_lagged_anchor,
};

/// Yields the fixed tx set once (here always empty ⇒ coinbase-only), then drains so the builder's
/// re-selection loop terminates.
struct OnceSelector(Option<Vec<Transaction>>);
impl TemplateTransactionSelector for OnceSelector {
    fn select_transactions(&mut self) -> Vec<Transaction> {
        self.0.take().unwrap_or_default()
    }
    fn reject_selection(&mut self, _tx_id: TransactionId) {}
    fn is_successful(&self) -> bool {
        true
    }
}

/// ADR-0040 P1-6 — the devnet demo's fixed ticket authority seed. Deterministic like everything else
/// in this path; the seeded leaf's `ticket_authority_pk_hash` binds to the matching key.
const PALW_DEMO_AUTHORITY_SEED: [u8; 32] = [0x9a; 32];

/// The demo batch's `batch_id`.
///
/// STILL OPEN (ADR-0040 §5.15, deliberately out of scope here): this is a LITERAL, so the demo batch is
/// NOT content-derived. Making it so requires seeding a real `PalwBatchManifestV1` — there is none — and
/// would move `batch_id`, which feeds `eligibility_hash` and therefore the grind in the mint. That is a
/// separate change with its own re-derivation, not a rename.
fn palw_demo_batch_id() -> Hash64 {
    Hash64::from_bytes([0x42; 64])
}

/// The demo batch holds exactly one leaf, at index 0. Both facts are load-bearing for
/// [`seeded_single_leaf_root`]: a one-leaf tree has depth 0, and the Merkle leaf node binds the index.
const PALW_DEMO_LEAF_INDEX: u32 = 0;
/// ADR-0045 D3-b — the reference mint's PCPB anchor epoch and committed roots. The anchor clears the
/// snapshot lag `k` so `anchor − k` is a real epoch (design memo §10.2); the roots are what
/// `seed_palw_demo_pcpb_context` commits there, so the mint-time clause 13 has a true equality to
/// check. This mint seeds the blob store directly and never reaches the acceptance arm, so clauses
/// 11/12 do not run on its leaf — but clause 13 does, and WITHOUT these rows the demo would simply
/// stop minting with `NotReady`, i.e. the quietest possible producer breakage (the failure mode
/// ADR-0040 §5.15.9's producer-inventory note calls out for exactly this file).
const PALW_DEMO_LEAF_ANCHOR_EPOCH: u64 = 4;
const PALW_DEMO_SNAPSHOT_ROOT: Hash64 = Hash64::from_bytes([0x7c; 64]);
const PALW_DEMO_ASSIGNMENT_ROOT: Hash64 = Hash64::from_bytes([0x7d; 64]);
const PALW_DEMO_PROOF_TYPE: u8 = 1;

/// The demo's mock leaf, for a given ticket-nullifier commitment.
///
/// Hoisted out of `palw_demo_mint_algo4_impl` (where it was a closure) so the ADR-0040 §5.15.9 producer
/// golden below can assert against the leaf the mint ACTUALLY seeds, rather than against a re-typed copy
/// of it that could drift from the real one without anything noticing.
fn palw_demo_leaf(ticket_commit: Hash64) -> PalwPublicLeafV1 {
    // Fixed devnet provider reward scripts (standard P2PKH ML-DSA class so the coinbase can pay them).
    let prov_a = p2pkh_mldsa87_spk(&[0xa0; 64]);
    let prov_b = p2pkh_mldsa87_spk(&[0xb0; 64]);
    PalwPublicLeafV1 {
        version: 1,
        batch_id: palw_demo_batch_id(),
        leaf_index: PALW_DEMO_LEAF_INDEX,
        job_nullifier: Hash64::from_bytes([9; 64]),
        ticket_nullifier_commitment: ticket_commit,
        model_profile_id: Hash64::from_bytes([1; 64]),
        runtime_class_id: Hash64::from_bytes([2; 64]),
        shape_id: 1,
        quantum_count: 1,
        proof_type: PALW_DEMO_PROOF_TYPE,
        provider_a_bond: TransactionOutpoint::new(Hash64::from_bytes([6; 64]), 0),
        provider_b_bond: TransactionOutpoint::new(Hash64::from_bytes([7; 64]), 0),
        provider_a_reward_script: prov_a,
        provider_b_reward_script: prov_b,
        // ADR-0040 P1-6 (AUTH-03): the leaf names the authority that may authorize its blocks, and
        // clause 7 checks it. A placeholder here would make the demo unmintable — which is the point.
        ticket_authority_pk_hash: kaspa_hashes::blake2b_512_keyed(
            kaspa_consensus_core::palw::PALW_AUTHORIZATION_DOMAIN,
            libcrux_ml_dsa::ml_dsa_87::generate_key_pair(PALW_DEMO_AUTHORITY_SEED).verification_key.as_ref(),
        ),
        private_match_commitment: Hash64::default(),
        // ADR-0045 D3-b: receipt DA Object V2. Object V1 requires the receipt-v3 fields to be zero
        // sentinels, and clause 11 re-derives one of them — so a V1 leaf is no longer a leaf any
        // honest path can produce (design memo §10.1).
        receipt_da_object_version: kaspa_consensus_core::palw::da::PALW_RECEIPT_DA_OBJECT_VERSION_V2,
        receipt_da_root: Hash64::from_bytes([0xda; 64]),
        receipt_da_object_len: 1,
        receipt_da_chunk_count: 1,
        receipt_v3_compute_set_id: Hash64::from_bytes([0xc5; 64]),
        receipt_v3_job_challenge: Hash64::from_bytes([9; 64]),
        receipt_v3_issued_epoch: PALW_DEMO_LEAF_ANCHOR_EPOCH,
        receipt_v3_expires_epoch: 1000,
        registered_epoch: 0,
        activation_epoch: 0,
        expiry_epoch: 1000,
        leaf_bond_sompi: 0,
        a_commit: Hash64::default(),
        a_commit_epoch: 0,
        provider_snapshot_root: PALW_DEMO_SNAPSHOT_ROOT,
        assignment_proof_root: PALW_DEMO_ASSIGNMENT_ROOT,
        dispatch_kind: kaspa_consensus_core::palw::PALW_DISPATCH_KIND_BEACON_ASSIGNED,
    }
}

/// Computes the `leaf_root` for a seeded single-leaf batch.
///
/// The shared derivation for the two seeding producers this tree has: the reference mint in this module
/// (`--palw-mine`) and the algo-4 E2E harness in `consensus/src/pipeline/virtual_processor/tests.rs`.
/// Both producers use this function so their root construction remains identical.
///
/// The projection zeroes `batch_id` before hashing, matching `manifest_leaf_root` in the miner: the
/// manifest commits to a batch's leaves before the batch has an id, since `leaf_root` is itself an input
/// to `content_id()`. This is deliberately not the same hash of the same leaf that
/// `resolve_palw_binding` uses for the eligibility draw (that one keeps `batch_id` populated). Two
/// hashes of one leaf serve distinct commitments; see ADR-0040 §5.15.12.
///
/// # Panics
/// If `leaf.leaf_index != 0`. A single-leaf batch has exactly one valid index, and the Merkle leaf node
/// binds it; a non-zero index here would silently produce a root nothing can open.
pub(crate) fn seeded_single_leaf_root(leaf: &PalwPublicLeafV1) -> Hash64 {
    assert_eq!(leaf.leaf_index, 0, "seeded_single_leaf_root models a ONE-leaf batch, whose only index is 0");
    let mut projected = leaf.clone();
    projected.batch_id = Hash64::default();
    palw_leaf_merkle_root(&[projected.leaf_hash()])
}

impl Consensus {
    /// See module docs. Errors (as a String) if not the devnet-palw preset, or if no finality-buried DNS
    /// anchor exists off the sink yet (mine more supporting blocks first).
    pub(crate) fn palw_demo_mint_algo4_impl(&self, miner_data: MinerData) -> Result<Block, String> {
        // ADR-0040 P0-1 — gate: **devnet-palw ONLY**.
        //
        // This path seeds a MOCK leaf, a certificate with EMPTY votes, and an `Active` batch view
        // directly into the real consensus stores, bypassing the whole Registering→Committed→Auditing
        // →Certified lifecycle. It had drifted to also accept `testnet-palw`, which is a *shared*
        // network running `palw_activation_daa_score = 0` — so forged provenance could reach a net with
        // other participants on it. Demo provenance belongs on a single-operator throwaway net only.
        //
        // Do NOT re-widen this to testnet-palw. The supported way to mint algo-4 on a shared net is the
        // real producer path (registration → k=2 receipts → auditor certificate), not seeded stores.
        let net = self.config.params.net;
        if net != DEVNET_PALW_PARAMS.net {
            return Err(format!("palw_demo_mint_algo4 is devnet-palw ONLY (net = {net:?})"));
        }
        let net_id = self.config.params.net.suffix().unwrap_or(0);
        let replica_bits = self.config.params.palw_lane_difficulty.genesis_replica_bits;
        let dns_params = self.config.params.dns_params.clone().ok_or("devnet-palw preset has no dns_params")?;

        let sink = self.get_sink();
        // Resolve the SAME finality-buried anchor the body check will (burial-only: headers + reachability).
        let anchor = resolve_palw_lagged_anchor(&self.storage.headers_store, &self.services.reachability_service, &dns_params, sink)
            .ok_or("no finality-buried DNS anchor off the sink yet — mine more algo-3 supporting blocks first")?;
        let anchor_header = self.storage.headers_store.get_header(anchor.anchor_hash).map_err(|e| format!("anchor header: {e:?}"))?;
        let anchor_facts = BeaconDnsAnchor {
            hash: anchor.anchor_hash,
            blue_score: anchor.anchor_blue_score,
            daa_score: anchor.anchor_daa_score,
            overlay_root: anchor_header.overlay_commitment_root,
        };
        let eligibility_beacon = anchor_header.palw_beacon_seed; // clause-9 lagged R_E

        let batch_id = palw_demo_batch_id();
        let leaf_index = PALW_DEMO_LEAF_INDEX;
        let proof_type = PALW_DEMO_PROOF_TYPE;
        let make_leaf = palw_demo_leaf;

        // Throwaway template off the sink to read the GHOSTDAG-fixed target interval (== daa_score).
        let tmpl0 = self
            .build_block_template(miner_data.clone(), Box::new(OnceSelector(Some(vec![]))), TemplateBuildMode::Infallible)
            .map_err(|e| format!("target-interval template: {e:?}"))?;
        let target_interval = tmpl0.block.header.daa_score;
        let expected_chain_commit =
            chain_commit(&anchor_facts.hash, &dns_finality_certificate_hash_v1(&anchor_facts), target_interval, net_id);

        // Grind the ticket nullifier so the clause-9 eligibility draw wins (max-easy bits ⇒ a few tries).
        let (nullifier, nonce) = {
            let mut b: u16 = 1;
            loop {
                let cand = Hash64::from_bytes([b as u8; 64]);
                let leaf = make_leaf(ticket_nullifier_commitment(&cand));
                let leaf_hash = leaf.leaf_hash();
                let digest = eligibility_hash(
                    net_id,
                    &eligibility_beacon,
                    &expected_chain_commit,
                    target_interval,
                    &batch_id,
                    leaf_index,
                    &leaf_hash,
                    &cand,
                );
                let cb = cand.as_byte_slice();
                let nonce = u64::from_le_bytes([cb[0], cb[1], cb[2], cb[3], cb[4], cb[5], cb[6], cb[7]]);
                if palw_eligibility_win(&digest, replica_bits, nonce, &cand) {
                    break (cand, nonce);
                }
                b += 1;
                if b >= 256 {
                    return Err("eligibility grind exhausted (target unexpectedly hard)".into());
                }
            }
        };

        // Seed the leaf + certificate content + a directly-Active batch view at the sink (== the algo-4
        // block's selected parent, which the body-stage ticket check reads).
        let leaf = make_leaf(ticket_nullifier_commitment(&nullifier));
        // kaspa-pq ADR-0040 §5.15.9 step (iii) — THE THIRD PRODUCER.
        //
        // `leaf_root` is the §5.15.4 uniform-depth Merkle root over the batch's ordered, `batch_id`-ZEROED
        // leaf hashes (this batch has exactly one leaf ⇒ depth 0 ⇒ the root is the finalize over the sole
        // leaf node). It was `Hash64::default()` — a literal that models nothing. Derived here so this
        // path stays a FAITHFUL producer model of what the miner/auditor emit, and so the value moves if
        // the construction ever moves.
        //
        // CORRECTION to the §5.15.9 producer inventory (verified in this tree): this mint does NOT
        // traverse the acceptance arm, so it is not broken by the M2 gate the way that note states. It
        // seeds `palw_store` DIRECTLY (`insert_leaf` below) and registers no manifest and no leaf-chunk
        // payload at all — `apply_palw_overlay_effect` is never reached from here. It is still a genuine
        // producer of a `leaf_root`, which is why it is moved; but the failure it was predicted to have is
        // structurally impossible, and the honest reason to fix it is fidelity, not liveness.
        //
        // Derived through the shared `seeded_single_leaf_root`, which is pinned by
        // `the_reference_mints_seeded_leaf_reduces_to_the_root_it_registers` below. Do NOT inline the
        // derivation here and do NOT substitute a literal: this value is consumed TWICE (the certificate
        // and the seeded lifecycle view), and those two must agree.
        let demo_leaf_root = seeded_single_leaf_root(&leaf);
        self.storage.palw_store.insert_leaf(batch_id, leaf_index, Arc::new(leaf)).map_err(|e| format!("insert_leaf: {e:?}"))?;
        // ADR-0045 D3-b: seed the clause-13 context in the same breath as the leaf. Nothing else
        // writes these rows for a leaf that never rode a chunk transaction, and a missing row is a
        // silent `NotReady`, not a loud error.
        {
            let mut batch = rocksdb::WriteBatch::default();
            let snapshot_epoch = PALW_DEMO_LEAF_ANCHOR_EPOCH - self.config.params.palw_snapshot_lag_epochs;
            let commitment = kaspa_consensus_core::palw::PalwSnapshotCommitment {
                snapshot_root: PALW_DEMO_SNAPSHOT_ROOT,
                assignment_root: PALW_DEMO_ASSIGNMENT_ROOT,
                total_bond: 1_000,
                provider_count: 2,
            };
            self.storage
                .palw_pcpb_store
                .set_snapshot_batch(&mut batch, snapshot_epoch, commitment)
                .map_err(|e| format!("demo snapshot seed: {e:?}"))?;
            // Static-audit C-01 — clause 13 resolves the snapshot by walking the minting block's own
            // chain, so the demo's fabricated anchor epoch has to sit on that chain too. The row goes
            // on the SINK, which is the minted block's selected parent, so the walk answers at its
            // first hop. Seeding only the epoch key would leave the walk refusing an epoch this short
            // devnet chain never closed — a silent `NotReady`, exactly what this block guards against.
            let sink = self.get_sink();
            self.storage
                .palw_pcpb_store
                .set_chain_state_batch(
                    &mut batch,
                    sink,
                    kaspa_consensus_core::palw::PalwPcpbChainStateV1 {
                        version: 1,
                        closed_snapshots: vec![(snapshot_epoch, commitment)],
                        prev_closer: None,
                    },
                )
                .map_err(|e| format!("demo pcpb chain seed: {e:?}"))?;
            let draw_epoch = PALW_DEMO_LEAF_ANCHOR_EPOCH + self.config.params.palw_post_commit_delta_epochs;
            let draw_seed = Hash64::from_bytes([0x7e; 64]);
            if self.storage.palw_beacon_store.beacon_seed_at(draw_epoch).map_err(|e| format!("demo seed read: {e:?}"))?.is_none() {
                self.storage
                    .palw_beacon_store
                    .set_beacon_seed_batch(&mut batch, draw_epoch, draw_seed)
                    .map_err(|e| format!("demo beacon seed: {e:?}"))?;
            }
            // The seed's fork-relative twin, appended to the sink's EXISTING row: the recurrence
            // fields feed the header overlay commitment, which the template builder and the validator
            // both read from this same row, so only `closed_seeds` may move.
            if let Some(state) = self.storage.palw_beacon_store.beacon_state(sink).map_err(|e| format!("demo state read: {e:?}"))? {
                let mut state = (*state).clone();
                if !state.closed_seeds.iter().any(|(e, _)| *e == draw_epoch) {
                    state.closed_seeds.push((draw_epoch, draw_seed));
                    self.storage
                        .palw_beacon_store
                        .set_state_batch(&mut batch, sink, Arc::new(state))
                        .map_err(|e| format!("demo beacon chain seed: {e:?}"))?;
                }
            }
            self.db.write(batch).map_err(|e| format!("demo pcpb seed write: {e:?}"))?;
        }
        let cert = PalwBatchCertificateV2 {
            version: PALW_BATCH_CERTIFICATE_VERSION_V2,
            batch_id,
            // No manifest is seeded for the demo batch, so there is nothing to derive a `manifest_hash`
            // from. Consensus only compares it in the Certificate ARM of `apply_palw_overlay_effect`,
            // which this path never reaches; `resolve_palw_binding` does not read it.
            manifest_hash: Hash64::default(),
            leaf_root: demo_leaf_root,
            audit_beacon_epoch: 0,
            audit_sample_root: Hash64::default(),
            passed_leaf_count: 1,
            rejected_leaf_bitmap_root: Hash64::default(),
            certificate_epoch: 0,
            activation_epoch: 0,
            expiry_epoch: 1000,
            auditor_set_commitment: Hash64::default(),
            // devnet demo: no real auditors, so no approving stake. Only meaningful at the VIRTUAL
            // coordinate (`verify_certificate_attestation` check 4); the body-stage view no longer reads
            // it at all (ADR-0040 CERT-TRUST).
            approving_stake: 0,
            votes: vec![],
        };
        let cert_hash = cert.hash();
        self.storage.palw_store.insert_certificate(cert_hash, Arc::new(cert)).map_err(|e| format!("insert_certificate: {e:?}"))?;
        let mut view = PalwBatchViewV1::new();
        view.batches.insert(
            batch_id,
            PalwBatchLifecycleV1 {
                status: PalwBatchStatus::Active,
                registration_epoch: 0,
                activation_not_before_epoch: 0,
                expiry_epoch: 1000,
                leaf_count: 1,
                chunk_count: 1,
                chunks_present: [1, 0, 0, 0],
                // The same derived root the certificate carries — `apply_manifest` copies
                // `manifest.leaf_root` into the lifecycle, so a faithful seeded view must agree with the
                // certificate rather than hold a placeholder (ADR-0040 §5.15.9 step (iii)).
                leaf_root: demo_leaf_root,
                cert_hash: Some(cert_hash),
                // ADR-0040 CERT-TRUST: inert. The body-stage fold never writes these and
                // `is_block_eligible_at` never reads them; the certificate window comes from the
                // attested blob. Seeded as 0 so this stays a FAITHFUL producer model.
                sponsor_tag_hi: 0,
                sponsor_tag_lo: 0,
                cert_approving_stake: 0,
                first_cert_daa: None,
                revoked_from_daa: None,
            },
        );
        self.storage.palw_overlay_view_store.set(sink, Arc::new(view)).map_err(|e| format!("set overlay view: {e:?}"))?;

        // Mint: fresh template off the sink, restamp as algo-4 + ticket fields, keeping the GHOSTDAG-derived
        // component work + the template-stamped beacon seed (S2 re-derives & authenticates both).
        let mut mb = self
            .build_block_template(miner_data, Box::new(OnceSelector(Some(vec![]))), TemplateBuildMode::Infallible)
            .map_err(|e| format!("algo-4 template: {e:?}"))?
            .block;
        let keep_hash_work = mb.header.blue_hash_work;
        let keep_compute_work = mb.header.blue_compute_work;
        let keep_beacon_seed = mb.header.palw_beacon_seed;
        mb.header.pow_algo_id = POW_ALGO_ID_PALW_REPLICA;
        mb.header.bits = replica_bits;
        mb.header.nonce = nonce;
        mb.header = mb.header.with_palw_fields(PalwHeaderFields {
            blue_hash_work: keep_hash_work,
            blue_compute_work: keep_compute_work,
            palw_beacon_seed: keep_beacon_seed,
            palw_batch_id: batch_id,
            palw_leaf_index: leaf_index,
            palw_ticket_nullifier: nullifier,
            palw_epoch_certificate_hash: cert_hash,
            palw_chain_commit: expected_chain_commit,
            palw_target_daa_interval: target_interval,
            palw_authorization_hash: Hash64::default(),
            palw_proof_type: proof_type,
            palw_spam_accumulator_commitment: Hash64::default(),
            palw_spam_nonce: 0,
            palw_compute_set_id: Default::default(),
            palw_compute_policy_id: Default::default(),
            palw_allocation_plan_id: Default::default(),
        }); // with_palw_fields re-finalizes header.hash over the full v3 preimage

        // ADR-0040 P1-6 — attach the per-block ticket authorization (construction == validation).
        // The demo owns the authority key deterministically, exactly as it owns the mock leaf.
        {
            use kaspa_consensus_core::palw::{
                PALW_AUTHORIZATION_MLDSA87_CONTEXT, PalwBlockAuthorizationV1, palw_header_preimage_commitment,
            };
            use libcrux_ml_dsa::ml_dsa_87 as mldsa;

            // ADR-0040 (AUTH-02): the commitment is TOTAL over the header preimage, with
            // `palw_authorization_hash` zeroed and `hash_merkle_root` replaced by `authed_root` (the
            // root over every tx EXCEPT the 0x38 authorization). Both substituted fields are the two the
            // authorization cannot bind without circularity, so it is safe — and required — to compute
            // this while `mb.header` still carries a zero authorization hash and the pre-auth merkle
            // root. EVERY OTHER HEADER FIELD MUST BE FINAL BY THIS POINT: retargeting the coinbase,
            // adding a parent, or recomputing any virtual-derived commitment after this line invalidates
            // the signature and wastes the ticket draw.
            let authed_root = kaspa_consensus_core::merkle::calc_hash_merkle_root(mb.transactions.iter());
            let commitment = palw_header_preimage_commitment(net_id, &mb.header, &authed_root);
            let kp = mldsa::generate_key_pair(PALW_DEMO_AUTHORITY_SEED);
            let mut auth = PalwBlockAuthorizationV1 {
                version: 1,
                batch_id,
                leaf_index,
                ticket_nullifier: nullifier,
                header_preimage_commitment: commitment,
                authority_public_key: kp.verification_key.as_ref().to_vec(),
                signature: vec![],
            };
            let digest = auth.signing_hash(net_id);
            auth.signature =
                mldsa::sign(&kp.signing_key, digest.as_bytes().as_slice(), PALW_AUTHORIZATION_MLDSA87_CONTEXT, [0x3cu8; 32])
                    .map_err(|e| format!("authorization sign: {e:?}"))?
                    .as_ref()
                    .to_vec();
            mb.header.palw_authorization_hash = auth.hash();
            // ADR-0040 (AUTH-TXSHAPE) — construction == validation, enforced structurally: the canonical
            // shape comes from the single consensus-core encoder
            // (`kaspa_consensus_core::palw::build_palw_authorization_transaction`), so this producer
            // cannot drift from `check_palw_block_authorization_shape` or clause 7. It MUST be the LAST
            // transaction (clause 7): nothing may be appended, sorted or reordered after this line.
            mb.transactions.push(kaspa_consensus_core::palw::build_palw_authorization_transaction(&auth));
            mb.header.hash_merkle_root = kaspa_consensus_core::merkle::calc_hash_merkle_root(mb.transactions.iter());
            mb.header.finalize();
        }
        Ok(mb.to_immutable())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw::{palw_leaf_merkle_depth, palw_leaf_merkle_proof, palw_verify_leaf_membership};

    /// Reference-mint ADR-0040 §5.15 vector (§5.15.9 step (iv)).
    ///
    /// The claim under test is the one the mint makes implicitly and nothing else checks: *the leaf this
    /// path seeds reduces to the `leaf_root` it registers.* Before this slice the answer was no — the
    /// certificate and the lifecycle view both carried `Hash64::default()`, a value no leaf reduces to.
    ///
    /// It is stated as a MEMBERSHIP check rather than as `root == palw_leaf_merkle_root(...)` on purpose.
    /// The latter re-computes with the same function on both sides and would still pass if the whole
    /// construction were wrong in the same way twice. The former runs the seeded leaf through the exact
    /// verifier the acceptance gate uses — `palw_verify_leaf_membership` — so it asserts a relationship
    /// between the leaf and the root, not an identity between two calls.
    ///
    /// Coverage note, stated plainly because the gap matters: this pins the DERIVATION
    /// (`seeded_single_leaf_root`) and the LEAF (`palw_demo_leaf`), which is why both were hoisted out of
    /// `palw_demo_mint_algo4_impl`. It does not execute the mint, so it cannot catch someone editing the
    /// mint's call site back to a literal — that path needs a live devnet-palw node and has no test in
    /// this tree. Making the derivation the single source of the value is the structural half of the
    /// defence; this test is the behavioural half.
    #[test]
    fn the_reference_mints_seeded_leaf_reduces_to_the_root_it_registers() {
        let leaf = palw_demo_leaf(ticket_nullifier_commitment(&Hash64::from_bytes([0x5b; 64])));
        let root = seeded_single_leaf_root(&leaf);

        // A one-leaf batch: depth 0, so the proof is empty and the root is the finalize over the sole
        // index-bound leaf node. There is no padding and no sibling to get wrong — which is precisely why
        // a placeholder here was so easy to leave in place unnoticed.
        assert_eq!(palw_leaf_merkle_depth(1), 0);

        let mut projected = leaf.clone();
        projected.batch_id = Hash64::default();
        let proof = palw_leaf_merkle_proof(&[projected.leaf_hash()], 0).expect("index 0 of a 1-leaf batch");
        assert!(proof.is_empty(), "depth 0 ⇒ no siblings");
        assert!(
            palw_verify_leaf_membership(&projected.leaf_hash(), leaf.leaf_index, 1, &proof, &root),
            "the leaf the reference mint seeds does not open the leaf_root it registers"
        );

        // The regression this replaces, named so it cannot come back quietly.
        assert_ne!(root, Hash64::default(), "leaf_root was a placeholder; a real leaf never reduces to zero");

        // The projection is load-bearing and is NOT the hash `resolve_palw_binding` uses for the
        // eligibility draw. Two different hashes of one leaf, both intentional (ADR-0040 §5.15.12).
        assert_ne!(
            projected.leaf_hash(),
            leaf.leaf_hash(),
            "the demo batch_id is non-zero, so the projected and unprojected leaf hashes must differ — \
             if these ever collapse, one of the two call sites has lost its projection"
        );
    }
}
