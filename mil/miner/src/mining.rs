//! PALW continuous-mining core (ADR-0039 §12) — the deterministic BODY of the algo-4 loop.
//!
//! An algo-4 block is won by a TICKET, not by hashing: a leaf is block-eligible for exactly one DAA
//! interval iff its one-shot draw `eligibility_hash(...)` clears the lane target, and the header nonce
//! is PINNED to the ticket nullifier (`nonce == low64(nullifier)`, I-3) so it is not a separate knob.
//!
//! # The grind is a REGISTRATION-time tool, not a mining loop
//!
//! It is tempting to read "the only mining freedom is the raw nullifier" as "the miner grinds
//! nullifiers until one wins". That is false on-chain, and getting it wrong produces a mining loop that
//! cannot mint. Clause 1 (`consensus/core/src/palw.rs`, `verify_palw_ticket`) requires
//!
//! ```text
//! ticket_nullifier_commitment(header.palw_ticket_nullifier) == leaf.ticket_nullifier_commitment
//! ```
//!
//! and the right-hand side is read from the leaf ALREADY ON CHAIN. So once a leaf is registered its
//! nullifier is fixed: **one leaf is one nullifier is one draw per interval.** A miner cannot re-roll.
//!
//! [`grind_eligibility`] varies the nullifier by rewriting `leaf.ticket_nullifier_commitment` on a
//! clone, which is legitimate ONLY before the leaf is published — it is how a producer chooses which
//! commitment to register. Calling it against a registered leaf yields a "winner" whose nullifier the
//! on-chain leaf does not commit to, and every block built from it fails clause 1.
//!
//! The mining-time counterpart is [`select_eligible_ticket`]: over the leaves this miner already owns
//! on chain, evaluate each one's single draw for the current interval and take a winner if there is
//! one. If none wins, this interval is simply not mined. What is expensive is not the draw — it is
//! MINTING a leaf (the k=2 inference behind `PalwMiner::produce_leaf`), which is why the
//! authority-ownership filter belongs before that, not before the draw.
//!
//! This module is those two halves plus the batch orchestration that ties the Phase-4b producers
//! (provider-bond → manifest → leaf-chunk → certificate) into one self-contained "stand up a batch and
//! win its first ticket" round.
//!
//! Everything here is pure and deterministic. The LIVE loop bindings — read the finality-buried DNS
//! anchor for the beacon seed + `chain_commit`, take the GHOSTDAG-fixed `target_daa_interval` off a
//! block template, submit the payloads as PALW TXs, and submit the minted algo-4 block — are the node's
//! job (Phase 5). This module gives that loop its brain and proves the whole round in-process against
//! the real consensus validators + the real `palw_eligibility_win` draw.

use kaspa_consensus_core::palw::{PalwPublicLeafV1, eligibility_hash, palw_eligibility_win, ticket_nullifier_commitment};
use kaspa_hashes::{Hash64, blake2b_512_keyed};

/// Miner-local domain for deriving a ticket authority's candidate nullifier sequence. NOT a consensus
/// input — the nullifier is just a `Hash64` the ticket holder picks; consensus only checks the draw
/// and the leaf's `ticket_nullifier_commitment`. Keyed so a holder's stream is reproducible by the
/// holder yet unpredictable to a third party reading the on-chain leaf (I-13 winner secrecy).
const TICKET_NULLIFIER_DOMAIN: &[u8] = b"misaka-palw-miner-ticket-v1";

/// The frozen per-block draw inputs a Header v3 binds (design §12.3), resolved from the finality-buried
/// DNS anchor (`beacon_seed`, the lagged `R_E`), the checkpoint (`chain_commit`), and the
/// GHOSTDAG-fixed target interval. In the live loop these come from the anchor header + a throwaway
/// template. In the live node they come from `ConsensusApi::palw_algo4_mint_facts`, which derives them
/// off the sink; here they are the caller-supplied context.
#[derive(Clone, Debug)]
pub struct EligibilityContext {
    pub network_id: u32,
    /// The anchor's `palw_beacon_seed` (clause-9 lagged `R_E`).
    pub beacon_seed: Hash64,
    /// `chain_commit(anchor, dns_cert, target_interval, net)` — the checkpoint the header commits to.
    pub chain_commit: Hash64,
    /// The GHOSTDAG-fixed DAA interval this draw targets (must equal the minted header's `daa_score`).
    pub target_daa_interval: u64,
    /// The lane difficulty (`bits`) the draw clears (`genesis_replica_bits` at genesis).
    pub replica_bits: u32,
}

/// A ticket whose raw nullifier makes the clause-9 draw win for a specific leaf.
#[derive(Clone, Debug)]
pub struct WinningTicket {
    /// Disclosed in the winning header (`palw_ticket_nullifier`); the leaf published only its commitment.
    pub raw_nullifier: Hash64,
    /// The header nonce, pinned to `low64(raw_nullifier)` (I-3).
    pub nonce: u64,
    /// The leaf carrying `ticket_nullifier_commitment(raw_nullifier)` — the on-chain ticket.
    pub leaf: PalwPublicLeafV1,
    pub leaf_hash: Hash64,
    /// 1-based number of candidates tried before this win (for logging / difficulty telemetry).
    pub tries: u64,
}

/// `low64(nullifier)` — the canonical algo-4 nonce the draw pins to the nullifier (I-3, non-grindable).
/// Mirrors the consensus `digest_low_u64` so a minted header's nonce matches `palw_eligibility_win`.
#[inline]
pub fn pinned_nonce(nullifier: &Hash64) -> u64 {
    let b = nullifier.as_byte_slice();
    u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])
}

/// A ticket authority's deterministic candidate-nullifier stream for one leaf: `candidate_i =
/// H_k(ticket-domain, secret ‖ i)`. Reproducible by the holder (same `secret`), unpredictable to a
/// third party (I-13). Feed to [`grind_eligibility`].
pub fn nullifier_sequence(secret: &[u8], count: u64) -> impl Iterator<Item = Hash64> + '_ {
    (0..count).map(move |i| {
        let mut p = Vec::with_capacity(secret.len() + 8);
        p.extend_from_slice(secret);
        p.extend_from_slice(&i.to_le_bytes());
        blake2b_512_keyed(TICKET_NULLIFIER_DOMAIN, &p)
    })
}

/// **REGISTRATION-TIME ONLY.** Choose which `ticket_nullifier_commitment` a not-yet-published leaf
/// should carry, by grinding the nullifier until the clause-9 draw wins for `base_leaf`.
///
/// Only the nullifier varies: for each candidate its commitment is swapped into a clone of `base_leaf`
/// (so the expensive k=2 inference that minted `base_leaf` runs ONCE, not per candidate), the leaf hash
/// + `eligibility_hash` are recomputed, and the pinned nonce `low64(nullifier)` is checked against the
/// lane target via [`palw_eligibility_win`]. Returns the first winner, or `None` if `candidates` is
/// exhausted. Pure + deterministic — a validator re-running the draw over the returned
/// `(leaf, nullifier)` gets the identical verdict.
///
/// # Do NOT call this against a registered leaf
///
/// Rewriting `ticket_nullifier_commitment` is what makes the grind work, and it is exactly what a leaf
/// on chain forbids: clause 1 compares `ticket_nullifier_commitment(disclosed_nullifier)` against the
/// commitment the ON-CHAIN leaf published, so a ground nullifier that the registered leaf does not
/// commit to fails validation no matter how good its draw is. A registered leaf gets ONE draw per
/// interval — use [`select_eligible_ticket`] for that. The only reason the seeded demo mint appears to
/// grind-then-win is that it writes the ground leaf into the store afterwards, which is not a thing a
/// miner on a shared network can do.
///
/// The winner's `raw_nullifier` must be PERSISTED alongside the registration: the miner needs it again,
/// possibly much later, to open its own leaf's commitment when the leaf finally becomes block-eligible.
pub fn grind_eligibility(
    ctx: &EligibilityContext,
    base_leaf: &PalwPublicLeafV1,
    candidates: impl IntoIterator<Item = Hash64>,
) -> Option<WinningTicket> {
    for (i, raw) in candidates.into_iter().enumerate() {
        let mut leaf = base_leaf.clone();
        leaf.ticket_nullifier_commitment = ticket_nullifier_commitment(&raw);
        let leaf_hash = leaf.leaf_hash();
        let digest = eligibility_hash(
            ctx.network_id,
            &ctx.beacon_seed,
            &ctx.chain_commit,
            ctx.target_daa_interval,
            &leaf.batch_id,
            leaf.leaf_index,
            &leaf_hash,
            &raw,
        );
        let nonce = pinned_nonce(&raw);
        if palw_eligibility_win(&digest, ctx.replica_bits, nonce, &raw) {
            return Some(WinningTicket { raw_nullifier: raw, nonce, leaf, leaf_hash, tries: i as u64 + 1 });
        }
    }
    None
}

/// A ticket this miner already holds ON CHAIN: a registered leaf, plus the raw nullifier whose
/// commitment that leaf published.
///
/// The nullifier is the secret half — the leaf discloses only `ticket_nullifier_commitment`, and I-13
/// winner secrecy lasts until the winning header discloses it at mint. It is chosen once, at
/// registration (see [`grind_eligibility`]), and must survive in the miner's own storage until the leaf
/// becomes block-eligible; there is no way to recover it from chain state.
#[derive(Clone, Debug)]
pub struct OwnedTicket {
    /// The leaf exactly as it is on chain — `batch_id` populated. Its `leaf_hash()` must be the one
    /// `resolve_palw_binding` computes, so this is NOT the `batch_id`-zeroed manifest projection.
    pub leaf: PalwPublicLeafV1,
    /// The nullifier the leaf's `ticket_nullifier_commitment` opens to.
    pub raw_nullifier: Hash64,
}

/// The single clause-9 draw a registered ticket gets for `ctx`'s interval. `None` if it does not win.
///
/// Also returns `None` — rather than a winner consensus would reject — when the leaf does not actually
/// commit to `raw_nullifier`. That mismatch means the stored secret does not belong to this leaf (a
/// mixed-up store, a leaf re-registered under a new commitment), and clause 1 would reject any block
/// built from it. Failing here keeps the miner from spending an interval on an unmintable ticket.
pub fn evaluate_ticket(ctx: &EligibilityContext, ticket: &OwnedTicket) -> Option<WinningTicket> {
    if ticket_nullifier_commitment(&ticket.raw_nullifier) != ticket.leaf.ticket_nullifier_commitment {
        return None;
    }
    let leaf_hash = ticket.leaf.leaf_hash();
    let digest = eligibility_hash(
        ctx.network_id,
        &ctx.beacon_seed,
        &ctx.chain_commit,
        ctx.target_daa_interval,
        &ticket.leaf.batch_id,
        ticket.leaf.leaf_index,
        &leaf_hash,
        &ticket.raw_nullifier,
    );
    let nonce = pinned_nonce(&ticket.raw_nullifier);
    palw_eligibility_win(&digest, ctx.replica_bits, nonce, &ticket.raw_nullifier).then(|| WinningTicket {
        raw_nullifier: ticket.raw_nullifier,
        nonce,
        leaf: ticket.leaf.clone(),
        leaf_hash,
        // A registered ticket is drawn once, not ground: one attempt, by construction.
        tries: 1,
    })
}

/// Mining-time ticket selection: the AUTH-03 ownership filter, then one draw per surviving ticket.
///
/// Tickets whose `leaf.ticket_authority_pk_hash` is not `authority_pk_hash` are dropped BEFORE they are
/// drawn. Winning such a draw would be worthless — clause 7 requires the block's authorization to be
/// signed by the authority the leaf named, and this miner cannot produce that signature — so a "win"
/// there would consume an interval and mint nothing.
///
/// Returns the first winner in iteration order. Callers wanting a deterministic choice across a
/// multi-ticket miner should pass a stably ordered collection; consensus does not care which of several
/// simultaneously-winning tickets is used, but reproducibility makes incident triage possible.
pub fn select_eligible_ticket<'a>(
    ctx: &EligibilityContext,
    authority_pk_hash: &Hash64,
    tickets: impl IntoIterator<Item = &'a OwnedTicket>,
) -> Option<WinningTicket> {
    tickets.into_iter().filter(|t| t.leaf.ticket_authority_pk_hash == *authority_pk_hash).find_map(|t| evaluate_ticket(ctx, t))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registration::tests::bind_test_v2_metadata;
    use crate::registration::{BatchPolicy, build_batch_manifest, build_leaf_chunk, build_provider_bond, restamp_leaves};
    use crate::{MiningJob, PalwMiner, ProviderRegistration};
    use kaspa_consensus_core::palw::{PALW_MAX_BATCH_LEAVES_V1, validate_palw_overlay_payload};
    use kaspa_consensus_core::tx::TransactionOutpoint;
    use kaspa_pq_validator_core::ValidatorKey;
    use misaka_palw::palw::{PalwRuntimeProfileV1, PalwSamplingParams, PalwTier};
    use misaka_palw::palw_replica::MockDeterministicRuntime;

    const NET: u32 = 0x9107;
    /// The max-easy genesis lane bits (`DEVNET_PALW`/`TESTNET_PALW`): `target_512 = target_256 << 256`
    /// with `target_256 ≈ 2^255`, i.e. ≈ 50 % win per candidate — a handful of tries suffices.
    const EASY_BITS: u32 = 0x207fffff;
    /// A tiny target (`target_256 = 1 ⇒ target_512 = 2^256`, win prob ≈ 2^-256): no candidate wins.
    const HARD_BITS: u32 = 0x03000001;

    fn h(b: u8) -> Hash64 {
        Hash64::from_bytes([b; 64])
    }

    fn profile() -> PalwRuntimeProfileV1 {
        PalwRuntimeProfileV1 {
            version: 1,
            tier: PalwTier::Quality,
            model_id: PalwTier::Quality.model_id(),
            tokenizer_hash: h(1),
            quantization_manifest_hash: h(2),
            runtime_image_hash: h(3),
            kernel_graph_hash: h(4),
            operation_table_hash: h(5),
            shape_table_hash: h(6),
            gpu_arch_class: 100,
            tensor_parallel_degree: 1,
            pipeline_parallel_degree: 1,
            deterministic_reduction: true,
            batch_invariant: true,
            speculative_decode: false,
            sampling: PalwSamplingParams::greedy(),
        }
    }

    fn miner() -> PalwMiner<MockDeterministicRuntime, MockDeterministicRuntime> {
        // ADR-0040 P0-4 (ECON-01): a leaf's reward scripts are emitted VERBATIM as coinbase outputs, so
        // leaf admission requires the exact 69-byte P2PKH ML-DSA-87 template. An arbitrary script is not
        // coinbase-representable and the leaf chunk is rejected.
        let spk = kaspa_consensus_core::dns_finality::p2pkh_mldsa87_spk(&[0xa0; 64]);
        PalwMiner::new(
            MockDeterministicRuntime::new(profile(), 3, 2),
            MockDeterministicRuntime::new(profile(), 3, 2),
            ProviderRegistration {
                provider_a_bond: TransactionOutpoint::new(h(6), 0),
                provider_b_bond: TransactionOutpoint::new(h(7), 0),
                provider_a_reward_script: spk.clone(),
                provider_b_reward_script: spk,
                ticket_authority_pk_hash: h(8),
                registered_epoch: crate::registration::tests::FIXTURE_REGISTRATION_EPOCH,
                activation_epoch: 4,
                expiry_epoch: 1000,
                leaf_bond_sompi: 0,
            },
        )
    }

    fn base_leaf(batch_id: Hash64) -> PalwPublicLeafV1 {
        miner()
            .produce_leaf(&MiningJob {
                batch_id,
                leaf_index: 0,
                job_set_descriptor: b"job".to_vec(),
                prompt: b"prompt".to_vec(),
                output_salt: [0x33; 32],
                job_nullifier: h(0x20),
                // The base nullifier is irrelevant — grind_eligibility swaps in each candidate's commitment.
                raw_ticket_nullifier: h(0),
                pcpb: crate::PcpbLeafFields::external(Hash64::default(), Hash64::default()),
            })
            .unwrap()
            .leaf
    }

    fn ctx(bits: u32) -> EligibilityContext {
        EligibilityContext {
            network_id: NET,
            beacon_seed: h(0xBE),
            chain_commit: h(0xCC),
            target_daa_interval: 42,
            replica_bits: bits,
        }
    }

    /// The mining core: grinding the nullifier over the max-easy genesis target finds a winner, and the
    /// winning ticket independently satisfies the exact `palw_eligibility_win` draw a validator runs —
    /// with the nonce pinned to `low64(nullifier)` and the leaf carrying the candidate's commitment.
    #[test]
    fn grind_finds_a_ticket_that_wins_the_real_draw() {
        let batch = h(0x10);
        let leaf = base_leaf(batch);
        let c = ctx(EASY_BITS);
        let ticket = grind_eligibility(&c, &leaf, nullifier_sequence(b"authority-secret", 128)).expect("easy target wins");

        // The nonce is pinned to the nullifier (I-3), and the leaf commits to the winning nullifier.
        assert_eq!(ticket.nonce, pinned_nonce(&ticket.raw_nullifier));
        assert_eq!(ticket.leaf.ticket_nullifier_commitment, ticket_nullifier_commitment(&ticket.raw_nullifier));

        // Re-run the EXACT consensus draw over the returned ticket — it must win.
        let digest = eligibility_hash(
            NET,
            &c.beacon_seed,
            &c.chain_commit,
            c.target_daa_interval,
            &batch,
            0,
            &ticket.leaf_hash,
            &ticket.raw_nullifier,
        );
        assert!(palw_eligibility_win(&digest, EASY_BITS, ticket.nonce, &ticket.raw_nullifier));
        assert!(ticket.tries >= 1);
    }

    /// Determinism: the same secret + context grinds to the identical winning ticket.
    #[test]
    fn grind_is_deterministic() {
        let leaf = base_leaf(h(0x10));
        let c = ctx(EASY_BITS);
        let a = grind_eligibility(&c, &leaf, nullifier_sequence(b"secret", 128)).unwrap();
        let b = grind_eligibility(&c, &leaf, nullifier_sequence(b"secret", 128)).unwrap();
        assert_eq!(a.raw_nullifier, b.raw_nullifier);
        assert_eq!(a.tries, b.tries);
    }

    /// An effectively-impossible target exhausts the candidate budget → no ticket (the caller waits or
    /// widens the search); it never fabricates a win.
    #[test]
    fn impossible_target_yields_no_ticket() {
        let leaf = base_leaf(h(0x10));
        assert!(grind_eligibility(&ctx(HARD_BITS), &leaf, nullifier_sequence(b"secret", 256)).is_none());
    }

    /// The whole self-contained round, end to end, validated against the REAL consensus gates: stand up
    /// a batch (provider-bond → manifest → leaf-chunk → auditor certificate), then grind the winning
    /// ticket for its first leaf. Every payload the loop would submit is accepted by the stateless
    /// validator under ONE content-derived batch id, and the ticket wins the real draw.
    #[test]
    fn full_self_contained_mining_round_end_to_end() {
        use crate::audit::{AuditRound, Auditor, QuorumPolicy, run_audit_round};

        // (1) Provider bond — the lifecycle's first payload.
        let prov_pubkey = ValidatorKey::from_seed([0x2C; 32]).public_key().to_vec();
        let (bond_byte, bond_payload, bond_out0) =
            build_provider_bond(prov_pubkey, h(0xA0), vec![h(1)], vec![(1, 4)], h(0xB0), 1_000, 10).unwrap();
        // ADR-0040 ECON-03: the E2E round trip asserts the TX-level rule (payload + locking output-0),
        // not just the payload — a bond with no backing must not pass the lifecycle's first step.
        assert_eq!(kaspa_consensus_core::palw::validate_palw_overlay_tx(bond_byte, &bond_payload, &[bond_out0]), Ok(()));

        // (2) Mine two leaves under a placeholder id, then fix them as a content-addressed batch.
        let m = miner();
        let mine = |idx: u32, nf: u8| {
            bind_test_v2_metadata(
                m.produce_leaf(&MiningJob {
                    batch_id: Hash64::default(),
                    leaf_index: idx,
                    job_set_descriptor: vec![idx as u8],
                    prompt: format!("p{idx}").into_bytes(),
                    output_salt: [0x33; 32],
                    job_nullifier: h(0x20 + idx as u8),
                    raw_ticket_nullifier: h(0xC0 + nf),
                    pcpb: crate::PcpbLeafFields::external(Hash64::default(), Hash64::default()),
                })
                .unwrap()
                .leaf,
            )
        };
        let leaves = vec![mine(0, 0), mine(1, 1)];
        let policy = BatchPolicy {
            registration_epoch: crate::registration::tests::FIXTURE_REGISTRATION_EPOCH,
            registration_lead_epochs: 2,
            audit_window_epochs: 1,
            active_window_epochs: 100,
            min_leaf_bond_sompi: 0,
            max_batch_leaves: PALW_MAX_BATCH_LEAVES_V1 as u32,
        };
        let (batch_id, (man_byte, man_payload)) = build_batch_manifest(&leaves, h(1), h(2), h(3), h(4), 0, &policy).unwrap();
        assert_eq!(validate_palw_overlay_payload(man_byte, &man_payload), Ok(()));
        // The certificate below MUST carry values DERIVED from this manifest, not placeholders:
        // `verify_certificate_attestation` cross-binds `cert.manifest_hash == manifest.content_id()`
        // and `cert.leaf_root == manifest.leaf_root` (consensus/src/processes/palw.rs:384-389). An
        // earlier revision of this "end-to-end" test used literals here, so it passed while the same
        // producer path would have been rejected on-chain with CertificateManifestMismatch /
        // CertificateLeafRootMismatch. Decode the manifest back and use its real fields, so the test
        // fails if the producer and the verifier ever disagree again.
        let manifest = <kaspa_consensus_core::palw::PalwBatchManifestV1 as borsh::BorshDeserialize>::try_from_slice(&man_payload)
            .expect("the manifest we just built must decode");
        assert_eq!(manifest.content_id(), batch_id, "batch_id is the manifest content id");

        // (3) Re-stamp under the content id + chunk on-chain.
        let restamped = restamp_leaves(batch_id, &leaves);
        let (chunk_byte, chunk_payload) =
            build_leaf_chunk(batch_id, 0, &restamped, &crate::registration::tests::dummy_witnesses(&restamped)).unwrap();
        assert_eq!(validate_palw_overlay_payload(chunk_byte, &chunk_payload), Ok(()));

        // (4) Auditor quorum certifies the batch. Each verdict is the OUTPUT of running the round
        // over the beacon-selected sample of THIS batch's leaves (AUDIT-EXEC-01) — there is no
        // longer a `pass: true` field to assert one into existence.
        struct ReproducesEverything;
        impl crate::audit::LeafAuditExecutor for ReproducesEverything {
            fn audit_leaf(&mut self, _leaf_index: u32, _leaf: &PalwPublicLeafV1) -> Result<bool, String> {
                Ok(true)
            }
        }
        let auditors: Vec<_> = [0x11u8, 0x22, 0x33]
            .into_iter()
            .zip(1u32..)
            .map(|(seed, index)| {
                let verdict = crate::audit::execute_audit_round(
                    &h(0x99),
                    &batch_id,
                    &restamped,
                    restamped.len() as u32,
                    &mut ReproducesEverything,
                )
                .expect("the round executes over this batch's leaves");
                (Auditor { key: ValidatorKey::from_seed([seed; 32]), bond: TransactionOutpoint::new(h(seed), index) }, verdict)
            })
            .collect();
        let slate: Vec<_> = auditors
            .iter()
            .map(|(auditor, _)| kaspa_consensus_core::palw::PalwCredentialStake {
                credential: auditor.bond.transaction_id,
                weight: 100,
                representative: auditor.bond,
            })
            .collect();
        let set_commit = kaspa_consensus_core::palw::auditor_set_commitment(&auditors.iter().map(|a| a.0.bond).collect::<Vec<_>>());
        let round = AuditRound {
            network_id: NET,
            batch_id,
            manifest_hash: manifest.content_id(),
            leaf_root: manifest.leaf_root,
            audit_beacon_epoch: 5,
            audit_sample_root: h(0x33),
            passed_leaf_count: 2,
            rejected_leaf_bitmap_root: h(0x44),
            certificate_epoch: 6,
            activation_epoch: 7,
            expiry_epoch: 13,
            auditor_set_commitment: set_commit,
        };
        let cert = run_audit_round(&round, &auditors, &slate, QuorumPolicy { num: 2, den: 3 }).unwrap();
        assert_eq!(validate_palw_overlay_payload(cert.subnetwork_byte, &cert.payload), Ok(()));

        // (5) Grind the winning ticket for the batch's first (restamped) leaf.
        let c = ctx(EASY_BITS);
        let ticket = grind_eligibility(&c, &restamped[0], nullifier_sequence(b"authority-secret", 128)).expect("ticket");
        let digest = eligibility_hash(
            NET,
            &c.beacon_seed,
            &c.chain_commit,
            c.target_daa_interval,
            &batch_id,
            0,
            &ticket.leaf_hash,
            &ticket.raw_nullifier,
        );
        assert!(palw_eligibility_win(&digest, EASY_BITS, ticket.nonce, &ticket.raw_nullifier), "the minted ticket wins the real draw");
        // The winning leaf resolves under the batch's content id (matches the chunk key the node stored).
        assert_eq!(ticket.leaf.batch_id, batch_id);
    }
    /// **C-1, pinned as a test.** A REGISTERED leaf gets exactly one draw, and the nullifier that draw
    /// uses is fixed by the commitment the leaf already published. `evaluate_ticket` agrees with the
    /// consensus draw on that one nullifier — it does not search.
    #[test]
    fn a_registered_ticket_draws_once_and_matches_the_consensus_verdict() {
        let batch_id = h(0x10);
        let c = ctx(EASY_BITS);
        // Registration time: pick the commitment to publish.
        let chosen = grind_eligibility(&c, &base_leaf(batch_id), nullifier_sequence(b"secret", 128)).expect("easy target wins");
        // The leaf as it now sits on chain, plus the secret the miner kept.
        let owned = OwnedTicket {
            leaf: restamp_leaves(batch_id, std::slice::from_ref(&chosen.leaf)).remove(0),
            raw_nullifier: chosen.raw_nullifier,
        };

        let won = evaluate_ticket(&c, &owned).expect("the registered ticket wins this interval");
        assert_eq!(won.raw_nullifier, chosen.raw_nullifier, "the draw must use the committed nullifier, not a new one");
        assert_eq!(won.nonce, pinned_nonce(&chosen.raw_nullifier), "I-3: the nonce is pinned, not searched");
        assert_eq!(won.tries, 1, "a registered ticket is drawn once — there is no re-roll");

        // Independently, the consensus draw reaches the same verdict over the same inputs.
        let digest = eligibility_hash(
            NET,
            &c.beacon_seed,
            &c.chain_commit,
            c.target_daa_interval,
            &owned.leaf.batch_id,
            owned.leaf.leaf_index,
            &owned.leaf.leaf_hash(),
            &owned.raw_nullifier,
        );
        assert!(palw_eligibility_win(&digest, EASY_BITS, won.nonce, &owned.raw_nullifier));

        // And on an impossible target the same ticket simply does not win — the miner sits the interval out.
        assert!(evaluate_ticket(&ctx(HARD_BITS), &owned).is_none());
    }

    /// The stored secret must actually open the leaf's commitment. A mismatch means the miner's ticket
    /// store and the chain disagree; clause 1 would reject any block built from it, so the draw refuses
    /// rather than reporting a win that cannot be minted.
    #[test]
    fn evaluate_ticket_refuses_a_nullifier_the_leaf_does_not_commit_to() {
        let batch_id = h(0x10);
        let c = ctx(EASY_BITS);
        let chosen = grind_eligibility(&c, &base_leaf(batch_id), nullifier_sequence(b"secret", 128)).expect("easy target wins");
        let leaf = restamp_leaves(batch_id, std::slice::from_ref(&chosen.leaf)).remove(0);

        let wrong = OwnedTicket { leaf: leaf.clone(), raw_nullifier: h(0xEE) };
        assert_ne!(ticket_nullifier_commitment(&wrong.raw_nullifier), leaf.ticket_nullifier_commitment);
        assert!(evaluate_ticket(&c, &wrong).is_none(), "a secret that does not open the leaf must never yield a winner");
    }

    /// **AUTH-03 filter.** A ticket whose leaf names some other ticket authority is dropped BEFORE the
    /// draw: this miner could never sign its clause-7 authorization, so winning it would consume the
    /// interval and mint nothing. The same ticket IS selected once the authority matches, which is what
    /// proves the filter — not the draw — is what excluded it.
    #[test]
    fn select_eligible_ticket_drops_tickets_this_miner_cannot_authorize() {
        let batch_id = h(0x10);
        let c = ctx(EASY_BITS);
        let chosen = grind_eligibility(&c, &base_leaf(batch_id), nullifier_sequence(b"secret", 128)).expect("easy target wins");
        let leaf = restamp_leaves(batch_id, std::slice::from_ref(&chosen.leaf)).remove(0);
        let owned = OwnedTicket { leaf: leaf.clone(), raw_nullifier: chosen.raw_nullifier };

        // The leaf's declared authority (the fixture registration's).
        let ours = leaf.ticket_authority_pk_hash;
        assert!(evaluate_ticket(&c, &owned).is_some(), "control: this ticket does win the draw");
        assert!(select_eligible_ticket(&c, &ours, [&owned]).is_some(), "the rightful authority selects it");

        // A different authority: same winning ticket, never selected.
        let stranger = h(0xF1);
        assert_ne!(stranger, ours);
        assert!(select_eligible_ticket(&c, &stranger, [&owned]).is_none(), "AUTH-03: an unsignable ticket is never drawn");
    }

    /// C-1 regression test. Grinding against a leaf that is
    /// already on chain produces a "winner" whose nullifier the registered leaf does NOT commit to.
    /// Clause 1 rejects every block built from it. The grind is a registration-time chooser, and this is
    /// the test that says so in executable form.
    #[test]
    fn grinding_a_registered_leaf_yields_a_ticket_clause_1_rejects() {
        let batch_id = h(0x10);
        let c = ctx(EASY_BITS);
        let registered = restamp_leaves(
            batch_id,
            &[grind_eligibility(&c, &base_leaf(batch_id), nullifier_sequence(b"secret", 128)).expect("easy target wins").leaf],
        )
        .remove(0);

        // Now do the wrong thing: grind again against the REGISTERED leaf with a different secret.
        let reground =
            grind_eligibility(&c, &registered, nullifier_sequence(b"a-second-secret", 128)).expect("the grind still finds a draw");

        // It "wins" — and is unmintable, because the on-chain leaf commits to a different nullifier.
        assert_ne!(
            ticket_nullifier_commitment(&reground.raw_nullifier),
            registered.ticket_nullifier_commitment,
            "the reground nullifier does not open the registered leaf's commitment"
        );
        let unmintable = OwnedTicket { leaf: registered, raw_nullifier: reground.raw_nullifier };
        assert!(evaluate_ticket(&c, &unmintable).is_none(), "the mining-time path must refuse what clause 1 would reject");
    }
}
