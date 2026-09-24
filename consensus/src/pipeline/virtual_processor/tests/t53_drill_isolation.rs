//! **T53 — drill isolation** (ADR-0152 §8.2 "the drill replay rule", `phase2-plan.md` §4 T53, P2-12):
//! what a testnet-12 drill chain produces, replayed into a PUBLIC testnet-12 test consensus, is
//! refused — by the missing UTXO or by the network domain — and a drill's coinbases do not mint
//! public testnet-12's outpoints.
//!
//! The public side is `t12_round_lane_e2e::t12_with_harness_cards` (the shipped ruleset, harness keys
//! on the eight cards, the premine imported on public testnet-12's own txids). The drill side is
//! `palw_t12_drill_params_v1` over a fixed test salt, with its drill-only keys
//! (`config::drill::PalwDrillKeyringV1`). Each refusal is shown against a twin that differs in the
//! one fact under test and is NOT refused for that reason, so no assertion passes vacuously:
//!
//! * **a registration** signed under the drill domain is refused at the signature; the same
//!   registration signed under this chain's domain passes that check;
//! * **its carrier** spends a drill genesis outpoint: `MissingTxOutpoints`; the twin spending the
//!   public float at the same index is not missing;
//! * **an attempt** mined on the drill is refused at the challenge (the domain), relabelling it to
//!   this chain's domain breaks its signature, and the bond it names is no bond here;
//! * **a conviction** whose receipt a drill signed is `PanelFalseValidReceiptUnverified`; the same
//!   seat's receipt under this chain's domain verifies and is refused only for its missing target;
//! * **coinbases**: at the same blue score, a drill paying a drill-only script and public testnet-12
//!   mint different coinbase txids — while two chains paying the SAME script mint the same one, which
//!   is the hazard drill-only keys exist for.
use super::t12_round_lane_e2e::t12_with_harness_cards;
use super::{OnetimeTxSelector, TestContext};
use crate::consensus::test_consensus::TestConsensus;
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::block::TemplateBuildMode;
use kaspa_consensus_core::coinbase::MinerData;
use kaspa_consensus_core::config::drill::{
    PalwDrillKeyRoleV1, PalwDrillKeyringV1, PalwDrillSaltV1, palw_t12_drill_fee_float_outpoint_v1, palw_t12_drill_genesis_utxos_v1,
    palw_t12_drill_premine_outpoint_v1,
};
use kaspa_consensus_core::config::params::{Params, palw_t12_drill_params_v1};
use kaspa_consensus_core::config::premine::{MAIN_PREMINE_INDEX, premine_outpoint_for};
use kaspa_consensus_core::config::{Config, ConfigBuilder};
use kaspa_consensus_core::errors::tx::TxRuleError;
use kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk;
use kaspa_consensus_core::palw_attempt_v2::{
    PALW_ATTEMPT_V2_MLDSA87_CONTEXT, PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, PalwAttemptV2Error,
    attempt_id_v2, challenge_v2, palw_network_domain_v2_for,
};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusParamsV2;
use kaspa_consensus_core::palw_offence_attribution_v1::{
    PALW_PANEL_FALSE_VALID_VERSION_V2, PalwFalseValidReceiptV1, PalwPanelFalseValidEvidenceV2,
};
use kaspa_consensus_core::palw_offence_v1::{
    PalwOffenceKindV1, PalwOffenceVerifyError, PalwPanelContradictionV1, palw_offence_evidence_digest_v1,
};
use kaspa_consensus_core::palw_panel_v2::{
    PALW_RECEIPT_V2_MLDSA87_CONTEXT, PalwReceiptVerdictV2, PalwSeatReceiptV2, palw_receipt_message_v2,
};
use kaspa_consensus_core::palw_state_v2::{
    PALW_BOND_REGISTRATION_V2_MLDSA87_CONTEXT, PALW_OPERATOR_POSSESSION_MLDSA87_CONTEXT, PalwBlockContextV2, PalwBondKeyV2,
    PalwChainStateV2, PalwConsensusObjectV2 as Obj, palw_bond_registration_message_v2, palw_operator_possession_message_v1,
};
use kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE;
use kaspa_consensus_core::tx::{MutableTransaction, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry};
use kaspa_hashes::Hash64;
use kaspa_muhash::MuHash;
use libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair;

/// The drill every test here replays from.
fn drill_salt() -> PalwDrillSaltV1 {
    PalwDrillSaltV1::from_bytes([0x53; 32]).expect("a test salt")
}

/// The domain a chain's V2 signatures are separated by — what the processor derives.
fn domain_of(params: &Params) -> Hash64 {
    palw_network_domain_v2_for(params.net.to_string().as_bytes(), Some(params.genesis.hash))
}

fn sign(keypair: &MLDSA87KeyPair, message: &[u8], context: &[u8]) -> Vec<u8> {
    libcrux_ml_dsa::ml_dsa_87::sign(&keypair.signing_key, message, context, [0x53u8; 32]).expect("sign").as_ref().to_vec()
}

fn verify(key: &[u8], message: &[u8], signature: &[u8], context: &[u8]) -> bool {
    kaspa_txscript::verify_mldsa87_with_context(key, message, signature, context).unwrap_or(false)
}

/// A chain at `config`'s genesis with `premine` imported as a node imports it.
fn at_genesis(config: &Config, premine: &[(TransactionOutpoint, UtxoEntry)]) -> TestContext {
    let consensus = TestConsensus::new(config);
    let mut imported = MuHash::new();
    consensus.append_imported_pruning_point_utxos(premine, &mut imported);
    consensus
        .import_pruning_point_utxo_set(config.params.genesis.hash, imported)
        .expect("the premine imports against the genesis commitment it was hashed into");
    TestContext::new(consensus)
}

/// **Public testnet-12**, at genesis, with the gate's view: the genesis state and a block point.
struct PublicT12 {
    ctx: TestContext,
    config: Config,
    bundle: PalwConsensusParamsV2,
    state: PalwChainStateV2,
    point: PalwBlockContextV2,
}

fn public_t12() -> PublicT12 {
    let (config, bundle, premine, _floats) = t12_with_harness_cards();
    let ctx = at_genesis(&config, &premine);
    let (_, state) =
        ctx.consensus.virtual_processor().palw_state_v2_store.read().load_tip(&bundle.state).unwrap().expect("the tip loads");
    let point = PalwBlockContextV2 {
        block: ctx.consensus.get_sink(),
        daa_score: ctx.consensus.get_virtual_daa_score() + 10,
        blue_score: 5,
        subsidy: 0,
    };
    PublicT12 { ctx, config, bundle, state, point }
}

impl PublicT12 {
    fn validate(&self, object: &Obj) -> Result<(), String> {
        self.ctx.consensus.virtual_processor().palw_v2_validate_objects(
            &self.state,
            &self.bundle.state,
            &self.point,
            std::slice::from_ref(object),
        )
    }

    fn accepted(&self, object: &Obj) -> Vec<Obj> {
        self.ctx.consensus.virtual_processor().palw_v2_accepted_objects_for_tests(
            &self.state,
            &self.bundle.state,
            &self.point,
            vec![object.clone()],
            self.point.block,
        )
    }

    fn domain(&self) -> Hash64 {
        domain_of(&self.config.params)
    }

    /// Genesis card `i`'s bond on this chain.
    fn card(&self, i: usize) -> PalwBondKeyV2 {
        self.bundle
            .genesis_objects
            .iter()
            .filter_map(|o| match o {
                Obj::BondRegistered { bond, .. } => Some(*bond),
                _ => None,
            })
            .nth(i)
            .expect("a genesis card")
    }
}

/// **The drill chain**: its params (the salt's genesis, the shipping rules), its domain and keyring.
struct Drill {
    params: Params,
    domain: Hash64,
    ring: PalwDrillKeyringV1,
}

fn drill() -> Drill {
    let params = palw_t12_drill_params_v1(&drill_salt());
    Drill { domain: domain_of(&params), ring: PalwDrillKeyringV1::new(drill_salt()), params }
}

/// A drill `BondRegistered` as `--palw-register-bond` builds it on the drill chain — bond key `n`,
/// which is also its operator identity, on the drill carrier's output 0 — signed under `domain`.
fn registration(d: &Drill, n: u32, carrier: Hash64, class: Hash64, domain: Hash64) -> Obj {
    let key = d.ring.key(PalwDrillKeyRoleV1::Bond, n);
    let keypair = key.keypair();
    let bond = PalwBondKeyV2(TransactionOutpoint::new(carrier, 0));
    let signed = kaspa_consensus_core::palw_lifecycle_objects_v2::palw_bond_registration_signed_key_v2(&bond);
    let collateral = 130_000 * kaspa_consensus_core::constants::SOMPI_PER_KASPA;
    let payout = Hash64::from_bytes(key.payload);
    let classes = std::collections::BTreeSet::from([class]);
    let message = palw_bond_registration_message_v2(domain, &signed, &key.pubkey, &key.pubkey, collateral, &payout, &classes);
    let possession = palw_operator_possession_message_v1(domain, &signed, &key.pubkey, &key.pubkey);
    let mut signature = sign(&keypair, message.as_byte_slice(), PALW_BOND_REGISTRATION_V2_MLDSA87_CONTEXT);
    signature.extend(sign(&keypair, possession.as_byte_slice(), PALW_OPERATOR_POSSESSION_MLDSA87_CONTEXT));
    Obj::BondRegistered {
        bond,
        pubkey: key.pubkey.clone(),
        operator_pubkey: key.pubkey,
        collateral,
        payout_payload: payout,
        capable_classes: classes,
        signature,
    }
}

/// **A drill registration is refused on public testnet-12 at its signature, and its carrier at its
/// inputs.** The registration verifies under the drill's domain and under nothing here; the twin
/// signed under this chain's domain gets past that check. The carrier spends the drill seat's fee
/// float, which public testnet-12 never minted; its twin spends the public float at the same index,
/// which exists.
#[tokio::test]
async fn t53_a_drill_registration_and_its_carrier_are_refused_on_public_testnet_12() {
    let public = public_t12();
    let d = drill();
    assert_ne!(d.domain, public.domain(), "the premise: the drill genesis moved the domain");
    let class = public.bundle.base_class_id;
    let carrier = Hash64::from_u64_word(0x53_C0);

    let replayed = registration(&d, 8, carrier, class, d.domain);
    let why = public.validate(&replayed).expect_err("a drill registration is refused");
    assert!(why.contains("is not signed by the key it declares"), "refused at the signature (the domain): {why}");
    assert!(public.accepted(&replayed).is_empty(), "and the walk drops it");
    let twin = registration(&d, 8, carrier, class, public.domain());
    if let Err(other) = public.validate(&twin) {
        assert!(!other.contains("is not signed by the key it declares"), "the twin clears the signature check: {other}");
    }

    // The carrier: the drill seat's float is not a UTXO here.
    let mempool = |spent: TransactionOutpoint| {
        let tx = Transaction::new(
            0,
            vec![TransactionInput::new(spent, vec![], 0, 1)],
            vec![TransactionOutput::new(1_000, p2pkh_mldsa87_spk(&d.ring.key(PalwDrillKeyRoleV1::Bond, 8).payload))],
            0,
            SUBNETWORK_ID_NATIVE,
            0,
            vec![],
        );
        public.ctx.consensus.validate_mempool_transaction(&mut MutableTransaction::from_tx(tx), &Default::default())
    };
    let drill_float = palw_t12_drill_fee_float_outpoint_v1(&drill_salt(), 0);
    assert!(matches!(mempool(drill_float), Err(TxRuleError::MissingTxOutpoints)), "a drill genesis outpoint is missing here");
    let public_float = premine_outpoint_for(public.config.params.net, MAIN_PREMINE_INDEX + 1);
    assert_eq!(public_float.index, drill_float.index, "the same index, on the other chain's txid");
    assert!(!matches!(mempool(public_float), Err(TxRuleError::MissingTxOutpoints)), "the twin's input exists");
}

/// **A drill attempt is refused on public testnet-12.** Its challenge is the drill domain's, so the
/// stateless check refuses it at this chain's (`ChallengeMismatch`) and passes it at the drill's;
/// relabelled to this chain's domain it no longer carries a valid signature (the id covers the
/// domain); and the bond it names — the drill seat on the drill premine — is no bond here.
#[tokio::test]
async fn t53_a_drill_attempt_is_refused_on_public_testnet_12() {
    let public = public_t12();
    let d = drill();
    let seat = d.ring.key(PalwDrillKeyRoleV1::Bond, 0);
    let bond = palw_t12_drill_premine_outpoint_v1(&drill_salt(), 0);
    let (pre_pow, timestamp, nonce, class) =
        (Hash64::from_u64_word(0x53_01), public.config.params.genesis.timestamp + 120_000, 7u64, public.bundle.base_class_id);
    let attempt_under = |domain: Hash64| PalwAttemptUnsignedV2 {
        version: PALW_ATTEMPT_V2_VERSION,
        network_domain: domain,
        challenge: challenge_v2(domain, pre_pow, timestamp, nonce, class, &bond),
        class_id: class,
        executor_bond: bond,
        executor_pubkey: seat.pubkey.clone(),
        operator_id: Hash64::from_u64_word(0x53_02),
        artifact_root: Hash64::from_u64_word(0x53_03),
        trace_root: Hash64::from_u64_word(0x53_04),
        output_root: Hash64::from_u64_word(0x53_05),
        pwu: 1,
        trace_manifest_root: Hash64::from_u64_word(0x53_06),
        trace_chunk_count: 1,
        trace_retention_daa: 1_000,
        execution_root: Hash64::from_u64_word(0x53_07),
    };
    let mined = {
        let attempt = attempt_under(d.domain);
        let signature = sign(&seat.keypair(), attempt_id_v2(&attempt).as_byte_slice(), PALW_ATTEMPT_V2_MLDSA87_CONTEXT);
        PalwAttemptEnvelopeV2 { attempt, signature }
    };
    mined.validate_stateless_v2(d.domain, pre_pow, timestamp, nonce).expect("valid on the drill it was mined on");
    mined.validate_signature_v2(verify).expect("signed by the drill seat");
    assert_eq!(
        mined.validate_stateless_v2(public.domain(), pre_pow, timestamp, nonce),
        Err(PalwAttemptV2Error::ChallengeMismatch),
        "refused on public testnet-12 at the challenge"
    );

    let mut relabelled = mined.clone();
    relabelled.attempt = attempt_under(public.domain());
    relabelled.validate_stateless_v2(public.domain(), pre_pow, timestamp, nonce).expect("the relabel fits this chain's challenge");
    assert_eq!(relabelled.validate_signature_v2(verify), Err(PalwAttemptV2Error::SignatureInvalid), "and breaks the drill signature");

    assert!(public.state.bond(&PalwBondKeyV2(bond)).is_none(), "the drill seat is no bond on public testnet-12");
    assert!(public.state.bond(&public.card(0)).is_some(), "the public card at the same index is");
    assert_eq!(public.card(0).0.index, bond.index);
}

/// **A drill conviction is refused on public testnet-12.** A `PanelFalseValidV2` whose receipt the
/// drill signed: against a drill seat it is refused outright, and even against a public seat with
/// that seat's own key its receipt verifies against nothing here — while the same seat's receipt
/// under this chain's domain verifies and is refused only because genesis holds no such claim.
#[tokio::test]
async fn t53_a_drill_conviction_is_refused_on_public_testnet_12() {
    let public = public_t12();
    let d = drill();
    let claim = Hash64::from_u64_word(0x53_C1A1);
    let contradiction = PalwPanelContradictionV1::CourtFraud { voided_daa: 7 };
    let offence = |accused: PalwBondKeyV2, keypair: &MLDSA87KeyPair, domain: Hash64| {
        let message = palw_receipt_message_v2(domain, claim, PalwReceiptVerdictV2::Valid, 0);
        let receipt = PalwSeatReceiptV2 {
            claim,
            verdict: PalwReceiptVerdictV2::Valid,
            seat_bond: accused,
            signed_daa: 0,
            signature: sign(keypair, message.as_byte_slice(), PALW_RECEIPT_V2_MLDSA87_CONTEXT),
        };
        let evidence = borsh::to_vec(&PalwPanelFalseValidEvidenceV2 {
            version: PALW_PANEL_FALSE_VALID_VERSION_V2,
            claim_id: claim,
            accused_seat: accused.0,
            receipt: PalwFalseValidReceiptV1::Full(receipt),
            contradiction: contradiction.clone(),
            prompt_ids_opening: None,
            reporter_reveal: Vec::new(),
        })
        .unwrap();
        Obj::ObjectiveOffence {
            kind: PalwOffenceKindV1::PanelFalseValidV2,
            accused,
            evidence_id: palw_offence_evidence_digest_v1(&evidence),
            evidence,
        }
    };

    // As the drill filed it: a drill seat, its drill key, the drill domain.
    let drill_seat = PalwBondKeyV2(palw_t12_drill_premine_outpoint_v1(&drill_salt(), 1));
    let filed = offence(drill_seat, &d.ring.key(PalwDrillKeyRoleV1::Bond, 1).keypair(), d.domain);
    public.validate(&filed).expect_err("a drill conviction is refused on public testnet-12");
    assert!(public.accepted(&filed).is_empty(), "and the walk drops it");

    // The domain alone refuses it: public card 1, its own key, the drill's domain.
    let card_key = TestConsensus::palw_v2_registry_keypair(1);
    let foreign = offence(public.card(1), card_key, d.domain);
    assert_eq!(public.validate(&foreign), Err(PalwOffenceVerifyError::PanelFalseValidReceiptUnverified.to_string()));
    // The twin under this chain's domain verifies, and is refused only for its missing target.
    let native = offence(public.card(1), card_key, public.domain());
    assert_eq!(public.validate(&native), Err(PalwOffenceVerifyError::NoTarget.to_string()));
}

/// **A drill's coinbase does not mint public testnet-12's outpoint.** At the first block's blue
/// score on both chains, the coinbase txid is a function of what it pays and not of the chain: the
/// SAME miner script on the drill and on public testnet-12 mints the SAME coinbase txid — the hazard
/// (a spend signed on one is a spend on the other, and nothing in the sighash names the chain). A
/// drill paying its drill-only heartbeat address mints a different txid, which is why a salted node
/// refuses any other address (`kaspad/src/palw_drill.rs`).
#[tokio::test]
async fn t53_drill_only_miner_scripts_mint_coinbases_public_testnet_12_never_mints() {
    let (public_config, _, public_premine, _) = t12_with_harness_cards();
    let public = at_genesis(&public_config, &public_premine);
    let d = drill();
    let drill_config = ConfigBuilder::new(d.params.clone())
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            // The harness's clock re-stamps nothing here, but the public side runs with the lane inert
            // (`t12_with_harness_cards`), and the two chains must differ in their genesis alone.
            p.evm_activation_daa_score = u64::MAX;
            p.palw_model_evm = None;
        })
        .apply_args(|c| c.palw_drill_genesis_salt = Some(drill_salt()))
        .build();
    let drill_premine: Vec<(TransactionOutpoint, UtxoEntry)> = palw_t12_drill_genesis_utxos_v1(&drill_salt()).into_iter().collect();
    let drill_chain = at_genesis(&drill_config, &drill_premine);
    assert_ne!(drill_config.params.genesis.hash, public_config.params.genesis.hash, "two chains");

    let coinbase = |ctx: &TestContext, payload: &[u8; 64]| {
        let template = ctx
            .consensus
            .build_block_template(
                MinerData::new(p2pkh_mldsa87_spk(payload), vec![]),
                Box::new(OnetimeTxSelector::new(Vec::new())),
                TemplateBuildMode::Standard,
            )
            .expect("a template");
        (template.block.header.blue_score, template.block.transactions[0].id())
    };
    let shared = [0x5Au8; 64];
    let drill_only = d.ring.key(PalwDrillKeyRoleV1::Heartbeat, 0).payload;

    let (public_score, public_shared) = coinbase(&public, &shared);
    let (drill_score, drill_shared) = coinbase(&drill_chain, &shared);
    assert_eq!(public_score, drill_score, "the same blue score on both chains");
    assert_eq!(public_shared, drill_shared, "the hazard: one script, one blue score, one coinbase txid — on two chains");
    let (_, drill_own) = coinbase(&drill_chain, &drill_only);
    assert_ne!(drill_own, public_shared, "a drill-only script mints a coinbase public testnet-12 never mints");
    let (_, other_own) = coinbase(&drill_chain, &d.ring.key(PalwDrillKeyRoleV1::Heartbeat, 1).payload);
    assert_ne!(drill_own, other_own, "and two drill-only miners at one blue score mint two");
}
