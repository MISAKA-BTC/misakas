//! **RFC-0009 stage C — public receipt redemption (V4) on the real virtual processor, with the miner and the builder DIFFERENT bonds.**
//!
//! `palw_receipt_spend_v4` is armed on a TEST copy of the params only (`None` on every preset). The executor's authorization is signed ONCE by
//! the executor bond's key and then handed around as an `RDA4` bundle: after that the executor's key is never touched again (the miner's PC is
//! off), and builders — other bonds with other keys — turn the bundle into receipt blocks of their own. What this pins, through the processor's
//! own methods (`palw_v2_unentitled_blues`, `palw_v2_receipt_v4_payouts`, the coinbase manager) and the fold:
//!
//! * a builder other than the miner redeems, and the miner leg goes to the EXECUTOR bond's registered payout while the builder keeps its fee;
//! * the miner offline: another builder redeems when the first is gone; two builders racing for one quantum are paid once;
//! * the builder's fee is at most the chain's cap and is carved out of the existing worker reward — nothing is minted, the Panel leg, the reserve
//!   and every other output are untouched, and the miner leg + fee equal what V3 pays the same block;
//! * a stale, mismatched or unregistered authorization/builder is refused (statelessly at the header stage, statefully in the entitlement);
//! * the spent-quantum ledger is branch-scoped: folding a spend marks the quantum used, reverting the delta (a reorg) gives it back, and the
//!   builder's identity is nowhere in consensus state (two builders' folds are the same state), nor does a receipt quantum touch the execution
//!   lane's round permits;
//! * V3 is unchanged: a V3 spend of the same claim is paid exactly as before (no split), and a V3 and a V4 spend of one quantum are one double spend.

use super::*;
use crate::model::stores::ghostdag::{GhostdagData, HashKTypeMap};
use kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for;
use kaspa_consensus_core::palw_freeprompt_v3::fp_quantum_ticket_v3;
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_pwu::palw_ticket_admits_v1;
use kaspa_consensus_core::palw_receipt_v4::{
    PALW_RECEIPT_V4_AUTH_MLDSA87_CONTEXT, PALW_RECEIPT_V4_BEACON_RULE_SLOT, PALW_RECEIPT_V4_BUILDER_FEE_CAP_BPS,
    PALW_RECEIPT_V4_SPEND_MLDSA87_CONTEXT, PALW_RECEIPT_V4_VERSION, PalwReceiptSpendEnvelopeV4, PalwReceiptSpendUnsignedV4,
    PalwRedemptionAuthBundleV4, PalwRedemptionAuthV4, fp_spend_id_v4, palw_receipt_v4_miner_script, palw_receipt_v4_split_v1,
    redeem_auth_id_v4, spend_challenge_v4,
};
use kaspa_consensus_core::palw_state_v2::{
    PALW_RECEIPT_TARGET_SEED_V1, PalwBlockContextV2, PalwBlockWorkV3, PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2,
    PalwConsensusObjectV2 as Obj, PalwMergedWorkV1, PalwPanelSeatV2, PalwPwuRuleV2, PalwStateParamsV2, PalwTransitionExtrasV1,
    apply_palw_transition_v2, apply_palw_transition_v7, revert_delta_v2,
};
use kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_RECEIPT_V3;
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use kaspa_consensus_core::{BlockHashMap, BlockHashSet, blockhash::BlockHashes};
use kaspa_hashes::Hash64;

fn h64(n: u64) -> Hash64 {
    Hash64::from_u64_word(n)
}

/// A bond with a key this test can sign for. Row 0 is the harness identity (the genesis bond `0xB0`); rows 1.. are the registry's keys.
struct Party {
    row: u64,
    bond: TransactionOutpoint,
    payout: Hash64,
}

impl Party {
    fn new(row: u64, payout: u64) -> Self {
        Self { row, bond: TransactionOutpoint::new(TransactionId::from_u64_word(0xB0 + row), 0), payout: h64(payout) }
    }
    fn key(&self) -> PalwBondKeyV2 {
        PalwBondKeyV2(self.bond)
    }
    fn kp(&self) -> &'static libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair {
        TestConsensus::palw_v2_registry_keypair(self.row)
    }
    fn pubkey(&self) -> Vec<u8> {
        self.kp().verification_key.as_ref().to_vec()
    }
    fn sign(&self, message: &[u8], context: &[u8]) -> Vec<u8> {
        libcrux_ml_dsa::ml_dsa_87::sign(&self.kp().signing_key, message, context, [0u8; 32]).expect("signs").as_ref().to_vec()
    }
}

/// What the executor authorizes (everything but the claim, which the rig knows).
#[derive(Clone, Copy)]
struct Authorizes {
    lo: u32,
    hi: u32,
    fee_bps: u16,
    expiry_daa: u64,
}

const WIDE: Authorizes = Authorizes { lo: 0, hi: 4, fee_bps: 500, expiry_daa: u64::MAX };

struct Rig {
    ctx: TestContext,
    net_domain: Hash64,
    tip: Hash64,
    beacon_block: Hash64,
    beacon_daa: u64,
    claim_id: Hash64,
    injected: PalwChainStateV2,
    inj_params: PalwStateParamsV2,
}

fn bond_obj(p: &Party, operator: u8) -> Obj {
    Obj::BondRegistered {
        bond: p.key(),
        pubkey: p.pubkey(),
        operator_pubkey: vec![operator; 8],
        collateral: 1u64 << 40,
        payout_payload: p.payout,
        capable_classes: Default::default(),
        signature: Vec::new(),
    }
}

impl Rig {
    /// The chain (fence armed on this test copy), mined to the claim's draw slot; the PALW state the receipt blocks are judged against holds the
    /// executor `e` and the builders `bs` as Active bonds and ONE certified (Final) free-prompt claim of `e`.
    async fn new(executor: &Party, builders: &[&Party]) -> Rig {
        let catalog = palw_v2_test_catalog();
        let bundle = {
            let registry = {
                let mut registry = kaspa_consensus_core::palw_fp_devnet_v3::palw_devnet_bond_registry_v1(
                    kaspa_consensus_core::palw_fp_devnet_v3::palw_v2_min_genesis_bonds_v1(),
                );
                registry[0].pubkey = TestConsensus::palw_v2_harness_pubkey();
                registry[0].operator_pubkey = vec![21u8; 8];
                for (i, row) in registry.iter_mut().enumerate().skip(1) {
                    row.pubkey = TestConsensus::palw_v2_registry_pubkey(i as u64);
                }
                registry
            };
            let mut b = kaspa_consensus_core::palw_fp_devnet_v3::palw_fp_bundle_with_windows_v3(
                h64(1),
                catalog.root(),
                h64(0xC0757),
                4_096,
                h64(0xA7),
                registry,
                &kaspa_consensus_core::palw_fp_devnet_v3::PALW_DEVNET_WINDOWS_V1,
            )
            .expect("the devnet-windowed harness bundle validates");
            b.class_catalog_root = catalog.root();
            for object in b.genesis_objects.iter_mut() {
                if let Obj::BondRegistered { collateral, .. } = object {
                    *collateral = 1u64 << 60;
                }
            }
            b
        };
        let maturity = bundle.freeprompt.receipt_maturity_daa();
        let config = ConfigBuilder::new(MAINNET_PARAMS)
            .skip_proof_of_work()
            .edit_consensus_params(|p| {
                p.palw_consensus_mode = PalwConsensusMode::ConsensusV2(bundle.clone());
                p.palw_audit_2026_09_11 = Some(kaspa_consensus_core::config::params::ForkActivation::always());
                *p = p.clone().with_palw_v2_cadence();
            })
            .build();
        // The fence is armed on THIS COPY of the params only; every preset keeps it `None`.
        let mut config = config;
        config.params.palw_receipt_spend_v4 = Some(kaspa_consensus_core::config::params::ForkActivation::always());
        let net_domain = palw_network_domain_v2_for(config.params.net.to_string().as_bytes(), Some(config.params.genesis.hash));
        let mut ctx = TestContext::new(TestConsensus::new(&config));

        let base_class = h64(1);
        let inj_params = PalwStateParamsV2::new(100, 1, 1, 1, 500, 1_000, base_class, 4, 1_000, 100, 1_000, 0)
            .unwrap()
            .with_fp_quanta(8, 64)
            .unwrap();
        let build_injected = |claim_id: Hash64| -> PalwChainStateV2 {
            let cx = |w: u64, daa: u64, blue: u64| PalwBlockContextV2 { block: h64(w), daa_score: daa, blue_score: blue, subsidy: 0 };
            let mut reg = vec![Obj::ClassRegistered {
                class_id: base_class,
                artifact_root: h64(11),
                slash_value_per_pwu: 5,
                pwu_rule: PalwPwuRuleV2::MaxPerAttempt(160),
                initial_target: u128::MAX / 2,
                share_permille: 1000,
                activation_daa: 0,
                admission: None,
            }];
            reg.push(bond_obj(executor, 21));
            for (i, b) in builders.iter().enumerate() {
                reg.push(bond_obj(b, 30 + i as u8));
            }
            let (s1, _) = apply_palw_transition_v2(&PalwChainStateV2::genesis(), &inj_params, &cx(1, 1, 1), &reg, None).unwrap();
            let commit = Obj::FreePromptCommitted {
                job_pin: kaspa_hashes::Hash64::default(),
                eval: None,
                claim: claim_id,
                class_id: base_class,
                bond: executor.key(),
                executor_pubkey: executor.pubkey(),
                work_leaves: 60,
                prompt_token_ids_hash: h64(0x7E),
                prompt_tokens: 0,
                prompt_token_ids: Vec::new(),
                decode_tokens_executed: 3,
                trace_root: h64(41),
                output_root: h64(42),
                execution_root: h64(43),
                trace_chunk_count: 4,
                trace_retention_daa: 999_999,
                consumed_prefix_state: kaspa_consensus_core::palw_freeprompt_v3::PalwFpPrefixStateV1::genesis(base_class),
            };
            let (s2, _) = apply_palw_transition_v2(&s1, &inj_params, &cx(2, 2, 2), &[commit], None).unwrap();
            let seats = vec![PalwPanelSeatV2 { bond: executor.key(), operator_id: h64(90) }];
            let (s3, _) = apply_palw_transition_v2(
                &s2,
                &inj_params,
                &cx(3, 3, 3),
                &[Obj::PanelBound { claim: claim_id, anchor: h64(77), seats }],
                None,
            )
            .unwrap();
            let (s4, _) = apply_palw_transition_v2(
                &s3,
                &inj_params,
                &cx(4, 4, 4),
                &[Obj::ReceiptLicensed { claim: claim_id, receipts: Vec::new() }],
                None,
            )
            .unwrap();
            let (s5, _) = apply_palw_transition_v2(&s4, &inj_params, &cx(5, 6, 5), &[], None).unwrap();
            s5
        };
        let probe = build_injected(h64(0xFC));
        let final_daa = match &probe.claim(&h64(0xFC)).expect("the fixture certifies a claim").phase {
            PalwClaimPhaseV2::Final { final_daa } => *final_daa,
            other => panic!("the fixture must reach Final, got {other:?}"),
        };
        let slot = final_daa + maturity;
        while ctx.consensus.get_virtual_daa_score() < slot + 2 {
            ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
        }
        let tip = ctx.consensus.get_sink();
        let beacon = ctx
            .consensus
            .virtual_processor()
            .palw_beacon_fact_of_candidate(tip, slot)
            .expect("the chain reached the draw slot, so a beacon derives");
        let claim_id = (0u64..2_000)
            .map(|i| h64(0xFC00 + i))
            .find(|cid| {
                palw_ticket_admits_v1(fp_quantum_ticket_v3(net_domain, beacon.beacon_block, *cid, 0), PALW_RECEIPT_TARGET_SEED_V1)
            })
            .expect("some claim id wins the receipt lottery under the seed target");
        let injected = build_injected(claim_id);
        Rig { ctx, net_domain, tip, beacon_block: beacon.beacon_block, beacon_daa: beacon.beacon_daa, claim_id, injected, inj_params }
    }

    /// **The executor's one signature**: an `RDA4` bundle the miner hands out and walks away from.
    fn authorization(&self, executor: &Party, a: Authorizes) -> Vec<u8> {
        let authorization = PalwRedemptionAuthV4 {
            version: PALW_RECEIPT_V4_VERSION,
            network_domain: self.net_domain,
            claim_id: self.claim_id,
            executor_bond: executor.bond,
            quantum_lo: a.lo,
            quantum_hi: a.hi,
            beacon_rule: PALW_RECEIPT_V4_BEACON_RULE_SLOT,
            builder_fee_bps: a.fee_bps,
            expiry_daa: a.expiry_daa,
        };
        let signature = executor.sign(redeem_auth_id_v4(&authorization).as_byte_slice(), PALW_RECEIPT_V4_AUTH_MLDSA87_CONTEXT);
        PalwRedemptionAuthBundleV4 { authorization, executor_pubkey: executor.pubkey(), signature }.encode()
    }

    /// **A builder's receipt block** from the bundle alone — the executor's key is not an input here. `nonce` makes siblings distinct.
    fn receipt_block(
        &self,
        builder: &Party,
        bundle_bytes: &[u8],
        quantum: u32,
        nonce: u64,
    ) -> kaspa_consensus_core::block::MutableBlock {
        let bundle = PalwRedemptionAuthBundleV4::decode(bundle_bytes).expect("a published bundle decodes");
        let mut r = self.ctx.consensus.build_block_with_parents_and_transactions(blockhash::NONE, vec![self.tip], vec![]);
        r.header.timestamp = self.ctx.simulated_time + 100 + nonce;
        r.header.nonce = nonce;
        r.header.pow_algo_id = POW_ALGO_ID_PALW_RECEIPT_V3;
        let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(&r.header);
        let a = &bundle.authorization;
        let spend = PalwReceiptSpendUnsignedV4 {
            version: PALW_RECEIPT_V4_VERSION,
            network_domain: self.net_domain,
            challenge: spend_challenge_v4(
                self.net_domain,
                pre_pow,
                r.header.timestamp,
                r.header.nonce,
                a.claim_id,
                quantum,
                &a.executor_bond,
                &builder.bond,
            ),
            claim_id: a.claim_id,
            quantum_index: quantum,
            beacon_block: self.beacon_block,
            executor_bond: a.executor_bond,
            builder_bond: builder.bond,
            builder_pubkey: builder.pubkey(),
            authorization: a.clone(),
            executor_pubkey: bundle.executor_pubkey.clone(),
            authorization_signature: bundle.signature.clone(),
        };
        let builder_signature = builder.sign(fp_spend_id_v4(&spend).as_byte_slice(), PALW_RECEIPT_V4_SPEND_MLDSA87_CONTEXT);
        r.header.palw_commitment = PalwReceiptSpendEnvelopeV4 { spend, builder_signature }.encode();
        r.header.finalize();
        r
    }

    async fn insert(&self, r: kaspa_consensus_core::block::MutableBlock) -> (Hash64, Result<(), String>) {
        let hash = r.header.hash;
        let verdict = self.ctx.consensus.validate_and_insert_block(r.to_immutable()).virtual_state_task.await;
        (hash, verdict.map(|_| ()).map_err(|e| e.to_string()))
    }

    /// The mergeset of a would-be chain block on `tip` that merges `reds` as in-DAA reds (the receipt lane's shape).
    fn merged(&self, reds: Vec<Hash64>) -> GhostdagData {
        GhostdagData::new(
            0,
            Default::default(),
            self.tip,
            BlockHashes::new(vec![self.tip]),
            BlockHashes::new(reds),
            HashKTypeMap::new(BlockHashMap::default()),
        )
    }

    fn point(&self) -> PalwBlockContextV2 {
        PalwBlockContextV2 { block: h64(0xC0DE), daa_score: self.beacon_daa, blue_score: 1 << 20, subsidy: 0 }
    }

    fn unentitled(&self, state: &PalwChainStateV2, gd: &GhostdagData) -> BlockHashSet {
        self.ctx.consensus.virtual_processor().palw_v2_unentitled_blues(state, gd, &BlockHashSet::default(), &self.point())
    }

    fn payouts(
        &self,
        state: &PalwChainStateV2,
        gd: &GhostdagData,
        unentitled: &BlockHashSet,
    ) -> BlockHashMap<kaspa_consensus_core::palw_receipt_v4::PalwReceiptV4Payout> {
        self.ctx.consensus.virtual_processor().palw_v2_receipt_v4_payouts(state, gd, &BlockHashSet::default(), unentitled)
    }

    fn fold(
        &self,
        state: &PalwChainStateV2,
        block: Hash64,
    ) -> (PalwChainStateV2, kaspa_consensus_core::palw_state_v2::PalwStateDeltaV2) {
        let envelope =
            PalwReceiptSpendEnvelopeV4::decode(&self.ctx.consensus.headers_store.get_header(block).unwrap().palw_commitment)
                .expect("decodes")
                .to_fold_envelope();
        let merged = vec![PalwMergedWorkV1 {
            carrying_block: block,
            work: PalwBlockWorkV3::ReceiptSpend(&envelope.spend),
            execution_key: Default::default(),
            subsidy: 0,
            escrow_carve: None,
            bits: 0,
            job_anchor: Hash64::default(),
        }];
        let point = PalwBlockContextV2 {
            block: h64(0xF01D),
            daa_score: state.last_point().map(|p| p.daa_score).unwrap_or(0) + 1,
            blue_score: state.last_point().map(|p| p.blue_score).unwrap_or(0) + 1,
            subsidy: 0,
        };
        let (next, delta, skips) = apply_palw_transition_v7(
            state,
            &self.inj_params,
            None,
            &point,
            &[],
            PalwBlockWorkV3::None,
            &merged,
            Default::default(),
            false,
            false,
            false,
            false,
            &PalwTransitionExtrasV1::default(),
        )
        .expect("the fold applies");
        assert!(skips.is_empty(), "the spend is applied, not skipped: {skips:?}");
        (next, delta)
    }
}

#[tokio::test]
async fn rfc9_c1_a_builder_other_than_the_miner_redeems_and_the_miner_is_paid_while_offline() {
    let executor = Party::new(0, 0x9A11);
    let (b1, b2, absent) = (Party::new(1, 0x9B01), Party::new(2, 0x9B02), Party::new(3, 0x9B03));
    let rig = Rig::new(&executor, &[&b1, &b2]).await;

    // The executor signs ONCE. From here the executor's key is not an input to anything below.
    let bundle = rig.authorization(&executor, WIDE);
    let decoded = PalwRedemptionAuthBundleV4::decode(&bundle).unwrap();
    decoded
        .validate_v4(rig.net_domain, |pk, msg, sig, ctx| {
            matches!(kaspa_txscript::verify_mldsa87_with_context(pk, msg, sig, ctx), Ok(true))
        })
        .expect("the published bundle is self-checking");

    // Two builders (other bonds, other keys) build receipt blocks from the bundle.
    let (r1, v1) = rig.insert(rig.receipt_block(&b1, &bundle, 0, 0xD1)).await;
    let (r2, v2) = rig.insert(rig.receipt_block(&b2, &bundle, 0, 0xD2)).await;
    assert!(
        v1.is_ok() || !v1.clone().unwrap_err().contains("PalwCarriage"),
        "builder 1's block gets through the header stage: {v1:?}"
    );
    assert!(
        v2.is_ok() || !v2.clone().unwrap_err().contains("PalwCarriage"),
        "builder 2's block gets through the header stage: {v2:?}"
    );

    // One quantum, two builders: paid once — the first in acceptance order — and the payout names the EXECUTOR's registered payout.
    let both = rig.merged(vec![r1, r2]);
    let un = rig.unentitled(&rig.injected, &both);
    assert!(un.contains(&r2) && !un.contains(&r1), "the race for one quantum has one winner: {un:?}");
    let payouts = rig.payouts(&rig.injected, &both, &un);
    assert_eq!(payouts.keys().copied().collect::<Vec<_>>(), vec![r1], "only the winner has a payout entry");
    let entry = &payouts[&r1];
    assert_eq!(
        entry.miner_script,
        palw_receipt_v4_miner_script(&executor.payout),
        "the miner leg is the executor bond's registered payout"
    );
    assert_eq!(entry.fee_bps, 500);

    // The first builder is gone (its block never merged): the OTHER builder redeems the same quantum and is paid.
    let only_b2 = rig.merged(vec![r2]);
    let un2 = rig.unentitled(&rig.injected, &only_b2);
    assert!(un2.is_empty(), "with builder 1 gone, builder 2's redemption is entitled: {un2:?}");
    assert_eq!(
        rig.payouts(&rig.injected, &only_b2, &un2).get(&r2).map(|p| p.miner_script.clone()),
        Some(palw_receipt_v4_miner_script(&executor.payout))
    );

    // A builder bond the chain does not hold, and one signing with another bond's key: stateless passes (shape and signatures are the
    // carried keys'), the ENTITLEMENT refuses — the builder must be an Active bond holding the key that signed the position.
    let (r_absent, va) = rig.insert(rig.receipt_block(&absent, &bundle, 0, 0xD3)).await;
    assert!(va.is_ok() || !va.clone().unwrap_err().contains("PalwCarriage"), "{va:?}");
    assert!(
        rig.unentitled(&rig.injected, &rig.merged(vec![r_absent])).contains(&r_absent),
        "an unregistered builder bond redeems nothing"
    );
    let impostor = Party { row: 2, bond: b1.bond, payout: b1.payout }; // claims builder 1's bond, signs with builder 2's key
    let (r_imp, vi) = rig.insert(rig.receipt_block(&impostor, &bundle, 0, 0xD4)).await;
    assert!(vi.is_ok() || !vi.clone().unwrap_err().contains("PalwCarriage"), "{vi:?}");
    assert!(
        rig.unentitled(&rig.injected, &rig.merged(vec![r_imp])).contains(&r_imp),
        "a builder bond cannot be named with another key"
    );

    // An authorization signed for a different executor bond than the claim's: the claim's executor is the only one who can authorize.
    let wrong_exec = Party { row: 1, bond: b1.bond, payout: b1.payout };
    let foreign = rig.authorization(&wrong_exec, WIDE);
    let (r_foreign, vf) = rig.insert(rig.receipt_block(&b2, &foreign, 0, 0xD5)).await;
    assert!(vf.is_ok() || !vf.clone().unwrap_err().contains("PalwCarriage"), "{vf:?}");
    assert!(
        rig.unentitled(&rig.injected, &rig.merged(vec![r_foreign])).contains(&r_foreign),
        "only the claim's executor authorizes its redemption"
    );

    // An expired authorization: not entitled.
    let stale = rig.authorization(&executor, Authorizes { expiry_daa: 1, ..WIDE });
    let (r_stale, vs) = rig.insert(rig.receipt_block(&b1, &stale, 0, 0xD6)).await;
    assert!(vs.is_ok() || !vs.clone().unwrap_err().contains("PalwCarriage"), "{vs:?}");
    assert!(rig.unentitled(&rig.injected, &rig.merged(vec![r_stale])).contains(&r_stale), "an expired authorization pays nobody");
}

#[tokio::test]
async fn rfc9_c2_the_header_stage_refuses_a_fee_above_the_cap_a_quantum_outside_the_range_and_forged_signatures() {
    let executor = Party::new(0, 0x9A11);
    let b1 = Party::new(1, 0x9B01);
    let rig = Rig::new(&executor, &[&b1]).await;
    // Refused at the header stage, and for the NAMED reason: a block refused for some other cause would pass a looser check.
    let refused = |verdict: &Result<(), String>, what: &str, reason: &str| {
        let text = verdict.clone().expect_err(&format!("{what} must be refused"));
        assert!(text.to_lowercase().contains(&reason.to_lowercase()), "{what}: expected a refusal for {reason:?}, got: {text}");
    };
    // The cap is the chain's, not the miner's: 1,001 bps is refused whoever signed it.
    let greedy = rig.authorization(&executor, Authorizes { fee_bps: PALW_RECEIPT_V4_BUILDER_FEE_CAP_BPS + 1, ..WIDE });
    refused(&rig.insert(rig.receipt_block(&b1, &greedy, 0, 0xE1)).await.1, "a fee above the cap", "bps");
    // A quantum the authorization does not cover.
    let narrow = rig.authorization(&executor, Authorizes { lo: 1, hi: 4, ..WIDE });
    refused(
        &rig.insert(rig.receipt_block(&b1, &narrow, 0, 0xE2)).await.1,
        "a quantum outside the authorized range",
        "outside the authorized range",
    );
    // A forged authorization: the executor's key, somebody else's signature.
    let honest = rig.authorization(&executor, WIDE);
    let mut forged = PalwRedemptionAuthBundleV4::decode(&honest).unwrap();
    forged.signature = b1.sign(redeem_auth_id_v4(&forged.authorization).as_byte_slice(), PALW_RECEIPT_V4_AUTH_MLDSA87_CONTEXT);
    refused(&rig.insert(rig.receipt_block(&b1, &forged.encode(), 0, 0xE3)).await.1, "a forged authorization signature", "signature");
    // A relay that alters the builder's block (the authorization's fee, after signing): the builder's signature covers it.
    let mut tampered = rig.receipt_block(&b1, &honest, 0, 0xE4);
    let mut envelope = PalwReceiptSpendEnvelopeV4::decode(&tampered.header.palw_commitment).unwrap();
    envelope.spend.authorization.builder_fee_bps = 1;
    tampered.header.palw_commitment = envelope.encode();
    tampered.header.finalize();
    refused(&rig.insert(tampered).await.1, "an authorization altered after it was signed", "signature");
    // The honest block of the same builder is fine.
    let ok = rig.insert(rig.receipt_block(&b1, &honest, 0, 0xE5)).await.1;
    assert!(ok.is_ok() || !ok.clone().unwrap_err().to_lowercase().contains("palw"), "{ok:?}");
}

#[tokio::test]
async fn rfc9_c3_the_builder_fee_is_carved_out_of_the_worker_reward_and_nothing_else_moves() {
    use kaspa_consensus_core::coinbase::{BlockRewardData, MinerData};
    let executor = Party::new(0, 0x9A11);
    let b1 = Party::new(1, 0x9B01);
    let rig = Rig::new(&executor, &[&b1]).await;
    let vp = rig.ctx.consensus.virtual_processor();
    let bundle = rig.authorization(&executor, WIDE);
    let (r1, _) = rig.insert(rig.receipt_block(&b1, &bundle, 0, 0xF1)).await;
    let gd = rig.merged(vec![r1]);
    let unentitled = rig.unentitled(&rig.injected, &gd);
    assert!(unentitled.is_empty());
    let payouts = rig.payouts(&rig.injected, &gd, &unentitled);

    let a_daa = rig.ctx.consensus.headers_store.get_header(r1).unwrap().daa_score;
    let subsidy = vp.coinbase_manager.calc_block_subsidy(a_daa);
    let carve = vp.fee_split_at(a_daa);
    let builder_script = kaspa_txscript::pay_to_script_hash_script(b"rfc9-c3-builder-block-miner");
    let tip_script = kaspa_txscript::pay_to_script_hash_script(b"rfc9-c3-tip-miner");
    let mut rewards = BlockHashMap::default();
    rewards.insert(rig.tip, BlockRewardData::new(subsidy, 0, 0, tip_script));
    rewards.insert(r1, BlockRewardData::new(subsidy, 0, 0, builder_script.clone()));
    let miner = MinerData::new(builder_script.clone(), vec![]);
    let build = |v4: &BlockHashMap<kaspa_consensus_core::palw_receipt_v4::PalwReceiptV4Payout>| {
        vp.coinbase_manager
            .expected_coinbase_transaction(
                a_daa,
                subsidy,
                miner.clone(),
                &gd,
                &rewards,
                &BlockHashSet::default(),
                &[],
                carve.as_ref(),
                (0, 0),
                0,
                &unentitled,
                true,
                &Default::default(),
                &Default::default(),
                v4,
            )
            .expect("the coinbase builds")
            .tx
    };
    let v3_like = build(&BlockHashMap::default());
    let v4 = build(&payouts);
    let by_script = |tx: &kaspa_consensus_core::tx::Transaction| {
        let mut m: std::collections::BTreeMap<Vec<u8>, u64> = Default::default();
        for o in &tx.outputs {
            *m.entry(o.script_public_key.script().to_vec()).or_default() += o.value;
        }
        m
    };
    let part = match &carve {
        Some(fs) => kaspa_consensus_core::dns_finality::split_block_subsidy(subsidy, fs).worker_base_sompi,
        None => subsidy,
    };
    let (leg, fee) = palw_receipt_v4_split_v1(part, 500);
    assert!(leg > 0 && fee > 0);
    // (1) the fee is at most the cap's share of the worker reward it is carved out of;
    assert!(
        u128::from(fee) * 10_000 <= u128::from(part) * u128::from(PALW_RECEIPT_V4_BUILDER_FEE_CAP_BPS),
        "builder fee {fee} of {part} exceeds the cap"
    );
    // (2) no value moves in or out of the block: the totals are V3's;
    assert_eq!(v4.outputs.iter().map(|o| o.value).sum::<u64>(), v3_like.outputs.iter().map(|o| o.value).sum::<u64>());
    // (3) the miner leg is exactly the executor's registered payout, and the builder's block keeps what V3 paid less that leg;
    let executor_script = palw_receipt_v4_miner_script(&executor.payout);
    let (v4_map, v3_map) = (by_script(&v4), by_script(&v3_like));
    assert_eq!(v4_map.get(executor_script.script()).copied().unwrap_or(0), leg);
    // (4) every OTHER output is untouched — the Panel leg, the reserve, the tip block's reward and the rest are the same pair for pair.
    let mut folded_back = v4_map.clone();
    let paid = folded_back.remove(executor_script.script()).unwrap();
    *folded_back.get_mut(builder_script.script()).expect("the builder is paid") += paid;
    assert_eq!(folded_back, v3_map, "moving the leg back to the builder reproduces V3's coinbase exactly: nothing else changed");
    // (5) a zero-fee authorization hands the whole worker share to the executor; 1 bps rounds toward the miner; the split always sums.
    for bps in [0u16, 1, 500, 1_000] {
        let (l, f) = palw_receipt_v4_split_v1(part, bps);
        assert_eq!(l + f, part, "bps {bps}");
    }
    assert_eq!(palw_receipt_v4_split_v1(part, 0), (part, 0));
    // (6) the fence-off twin: with no payout map the V3 expression is what is paid (a V3 receipt block, and every block below the fence).
    assert_eq!(by_script(&build(&BlockHashMap::default())), v3_map);
}

#[tokio::test]
async fn rfc9_c4_the_spent_quantum_is_branch_scoped_the_builder_is_not_state_and_a_quantum_is_not_a_round_permit() {
    let executor = Party::new(0, 0x9A11);
    let (b1, b2) = (Party::new(1, 0x9B01), Party::new(2, 0x9B02));
    let rig = Rig::new(&executor, &[&b1, &b2]).await;
    let bundle = rig.authorization(&executor, WIDE);
    let (r1, _) = rig.insert(rig.receipt_block(&b1, &bundle, 0, 0xA1)).await;
    let (r2, _) = rig.insert(rig.receipt_block(&b2, &bundle, 0, 0xA2)).await;

    // The branch that accepted builder 1's spend: the quantum is spent there, so a later redemption of it is unentitled and unpaid.
    let (spent, delta) = rig.fold(&rig.injected, r1);
    assert!(spent.safe_weight() > rig.injected.safe_weight(), "the redeemed quantum's weight is credited");
    let again = rig.merged(vec![r2]);
    let un = rig.unentitled(&spent, &again);
    assert!(un.contains(&r2), "a quantum used on this branch is not spendable again");
    assert!(rig.payouts(&spent, &again, &un).is_empty(), "…and pays nothing");
    // The sibling branch that never saw builder 1's block: the same quantum is still spendable there.
    let sibling = rig.unentitled(&rig.injected, &again);
    assert!(sibling.is_empty(), "on the other branch the quantum is unspent: {sibling:?}");
    assert_eq!(rig.payouts(&rig.injected, &again, &sibling).len(), 1);

    // A reorg out of builder 1's branch: reverting its delta gives the state — and the quantum — back, bit for bit.
    let back = revert_delta_v2(&spent, &delta, &rig.inj_params).expect("the delta reverts");
    assert_eq!(back.state_root(), rig.injected.state_root(), "a reorg returns usage and weight");
    assert!(rig.unentitled(&back, &again).is_empty());

    // The builder's identity is nowhere in consensus state: folding builder 2's block instead gives the SAME state.
    let (spent_by_b2, _) = rig.fold(&rig.injected, r2);
    assert_eq!(spent_by_b2.state_root(), spent.state_root(), "who built the block is a payout fact, not a state fact");
    assert_eq!(spent_by_b2.safe_weight(), spent.safe_weight());

    // A receipt quantum is not an execution-lane round permit: redeeming one touches none of the lane's permit/final accounting.
    for span in 0..8u64 {
        assert_eq!(spent.round_permits_accepted(span), rig.injected.round_permits_accepted(span), "span {span}");
    }
    assert_eq!(spent.round_finals().1.len(), rig.injected.round_finals().1.len());
    assert_eq!(spent.round_finals().0, rig.injected.round_finals().0);
}

#[tokio::test]
async fn rfc9_c5_v3_is_unchanged_and_a_v3_and_a_v4_spend_of_one_quantum_are_one_double_spend() {
    let executor = Party::new(0, 0x9A11);
    let b1 = Party::new(1, 0x9B01);
    let rig = Rig::new(&executor, &[&b1]).await;
    // A V3 receipt block of the claim by its executor (V3 requires the producer to BE the executor).
    let mut v3 = rig.ctx.consensus.build_block_with_parents_and_transactions(blockhash::NONE, vec![rig.tip], vec![]);
    v3.header.timestamp = rig.ctx.simulated_time + 400;
    v3.header.nonce = 0xB3;
    v3.header.pow_algo_id = POW_ALGO_ID_PALW_RECEIPT_V3;
    v3.header.palw_commitment =
        rig.ctx.consensus.palw_v3_test_receipt_carriage_for(&v3.header, true, rig.claim_id, 0, executor.bond, rig.beacon_block);
    v3.header.finalize();
    let (v3_hash, _) = rig.insert(v3).await;
    // Alone, the V3 spend is entitled and gets NO V4 payout entry — it is paid as V3 always was.
    let gd = rig.merged(vec![v3_hash]);
    let un = rig.unentitled(&rig.injected, &gd);
    assert!(un.is_empty(), "a V3 spend of a certified claim is still entitled with the V4 fence armed: {un:?}");
    assert!(rig.payouts(&rig.injected, &gd, &un).is_empty(), "…and is not split");
    // A V4 spend of the SAME quantum beside it: one ledger, paid once, whichever is first.
    let bundle = rig.authorization(&executor, WIDE);
    let (v4_hash, _) = rig.insert(rig.receipt_block(&b1, &bundle, 0, 0xB4)).await;
    let v3_first = rig.merged(vec![v3_hash, v4_hash]);
    let un = rig.unentitled(&rig.injected, &v3_first);
    assert!(
        un.contains(&v4_hash) && !un.contains(&v3_hash),
        "V3 first: V3 is paid, the V4 spend of the same quantum is the double spend"
    );
    let v4_first = rig.merged(vec![v4_hash, v3_hash]);
    let un = rig.unentitled(&rig.injected, &v4_first);
    assert!(un.contains(&v3_hash) && !un.contains(&v4_hash), "V4 first: V4 is paid (split), the V3 spend is the double spend");
    assert_eq!(rig.payouts(&rig.injected, &v4_first, &un).keys().copied().collect::<Vec<_>>(), vec![v4_hash]);
}
