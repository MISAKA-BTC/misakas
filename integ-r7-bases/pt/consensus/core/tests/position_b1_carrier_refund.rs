//! **The 2026-09-23 Position route matrix, P-B1, on testnet-12's own params and genesis: a
//! carrier-borne buy or seed that acceptance refuses is paid back to the payer — never kept.**
//!
//! A market move rides a transaction whose sink output is an `OP_RETURN`, so by the time any rule
//! that reads state speaks, the transaction is accepted and the MSK is out of circulation. The
//! route matrix measured the consequence (P8): C paid 1,000 MSK, the fold dropped C's buy, and C
//! held nothing and was refunded nothing. This file replays that block the way
//! `VirtualStateProcessor::palw_v2_accepted_objects_and_refunds` and the chain walk do — the lifecycle
//! walk over real carriers, the rehearsal that drops a refused object, `palw_model_carrier_refund_v1`
//! for what each dropped carrier is owed, and the transition handed those refunds — and states the
//! design as an invariant over every carrier: **the payer holds what it paid for, or is owed
//! exactly what it paid.** Stated that way it holds whichever rule refuses the move — for a carrier
//! whose change pays a P2PKH-ML-DSA-87 script (every carrier the shipped tools build), and while the
//! payout queue has a row for its refund: refunds count against `PALW_V2_MAX_PENDING_PAYOUTS`, and a
//! node refuses at its mempool and template a carrier the queue could not refund. A carrier with no
//! such output, or one mined anyway into a full queue, still burns, and the processor logs it.
//!
//! Run: `cargo test -p kaspa-consensus-core --test position_b1_carrier_refund`

use kaspa_consensus_core::config::params::{ForkActivation, Params};
use kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk;
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_lifecycle_objects_v2::{
    PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2, palw_lifecycle_objects_from_accepted_txs_v2, palw_model_carrier_refund_v1,
};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_model_market_v1::{PalwModelFeesV1, palw_model_buy_quote_with, palw_model_sink_spk_v1};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwCarrierRefundV1, PalwChainStateV2, PalwConsensusObjectV2 as Obj, PalwStateParamsV2,
    PalwTransitionExtrasV1, apply_palw_transition_v2_with_extras, palw_model_refund_payout_key_v1, palw_v2_apply_one_object_v1,
    palw_v2_pre_object_base_v1, revert_delta_v2,
};
use kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE;
use kaspa_consensus_core::tx::{Transaction, TransactionInput, TransactionOutpoint, TransactionOutput};
use kaspa_hashes::Hash64;

const MSK: u64 = 100_000_000;

fn h(v: u64) -> Hash64 {
    Hash64::from_u64_word(v)
}

fn network(suffix: u32) -> Params {
    Params::from(NetworkId::with_suffix(NetworkType::Testnet, suffix))
}

fn extras(p: &Params, daa: u64) -> PalwTransitionExtrasV1 {
    PalwTransitionExtrasV1 {
        model_lines_active: p.palw_model_lines_active_at(daa),
        model_benefits_active: p.palw_model_benefits_active_at(daa),
        evm_market_active: p.palw_model_evm_active_at(daa),
        model_leg_v2_active: p.palw_model_leg_v2_active_at(daa),
        model_seed_v2_active: p.palw_model_seed_v2_active_at(daa),
        artifact_root_ownership_active: p.palw_artifact_root_ownership_at(daa),
        audit_2026_09_23_active: p.palw_audit_2026_09_23_active_at(daa),
        // ADR-0152-adjacent (Activation Pool): resolved as the processor resolves it, so a top-up
        // carrier folds (testnet-12) or is refused as dormant (testnet-11).
        activation_pool: p.palw_activation_pool_at(daa),
        ..Default::default()
    }
}

/// A carrier as `misaka palw model-buy` / `model-seed` build it: the change back to the payer's own
/// P2PKH-ML-DSA-87 script at output 0, the sink at output 1, the object in the payload.
fn carrier(object: &Obj, payer: Hash64, line: Hash64, paid: u64, nonce: u32) -> Transaction {
    let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object: object.clone() }).unwrap();
    Transaction::new(
        0,
        vec![TransactionInput::new(TransactionOutpoint::new(h(0xF00D), nonce), vec![], 0, 1)],
        vec![
            TransactionOutput::new(3 * MSK, p2pkh_mldsa87_spk(&payer.as_bytes())),
            TransactionOutput::new(paid, palw_model_sink_spk_v1(&line)),
        ],
        0,
        SUBNETWORK_ID_PALW_LIFECYCLE,
        0,
        payload,
    )
}

fn buy(line: Hash64, holder: Hash64, msk_in: u64, min_units_out: u64) -> Obj {
    Obj::ModelBuy { line_id: line, holder, msk_in, min_units_out, sink_index: 1 }
}

struct Chain {
    p: Params,
    sp: PalwStateParamsV2,
    state: PalwChainStateV2,
    daa: u64,
    /// `false` replays the block as a build without P-B1 does — the testnet-11 path.
    refunds_armed: bool,
}

impl Chain {
    fn t12(refunds_armed: bool) -> Self {
        let p = network(12);
        let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { panic!("testnet-12 is ConsensusV2") };
        let sp = bundle.state.clone();
        let ctx = PalwBlockContextV2 { block: h(0x6E6E), daa_score: 0, blue_score: 0, subsidy: 0 };
        let (state, _) = apply_palw_transition_v2_with_extras(
            &PalwChainStateV2::genesis(),
            &sp,
            &ctx,
            &bundle.genesis_objects,
            None,
            false,
            false,
            false,
            false,
            &extras(&p, 0),
        )
        .expect("testnet-12's genesis folds");
        Chain { p, sp, state, daa: 0, refunds_armed }
    }

    fn flags(&self, daa: u64) -> (bool, bool, bool, bool) {
        let at = |f: Option<ForkActivation>| f.is_some_and(|f| f.is_active(daa));
        (
            at(self.p.palw_unavailable_abstains),
            self.p.palw_capability_bound_at(daa),
            at(self.p.palw_uncertified_weightless),
            at(self.p.palw_da_court),
        )
    }

    /// One chain block accepting `carriers`: the lifecycle walk, the rehearsal that drops what the
    /// fold refuses (and, armed, names what each dropped carrier is owed), then the transition.
    /// Returns the refunds the block paid.
    fn block(&mut self, carriers: &[Transaction]) -> Vec<PalwCarrierRefundV1> {
        let daa = self.daa + 1;
        let ctx = PalwBlockContextV2 { block: h(0x1000_0000 | daa), daa_score: daa, blue_score: daa, subsidy: 0 };
        let (ua, cb, uw, dc) = self.flags(daa);
        let mut ex = extras(&self.p, daa);
        let walk = palw_lifecycle_objects_from_accepted_txs_v2(carriers);
        assert_eq!(walk.objects.len(), carriers.len(), "every carrier rides: {:?}", walk.skipped);
        let mut folded = palw_v2_pre_object_base_v1(&self.state, &self.sp, &ctx, ua, cb, uw, dc, &ex).expect("pre-object base");
        let (mut accepted, mut refunds) = (Vec::new(), Vec::new());
        for carried in walk.objects {
            let tx = carriers.iter().find(|tx| tx.id() == carried.carrier).expect("the carrier");
            match palw_v2_apply_one_object_v1(&folded, &self.sp, &ctx, &carried.object, ua, cb, uw, dc, &ex) {
                Ok(next) => {
                    folded = next;
                    accepted.push(carried.object);
                }
                Err(_) if self.refunds_armed && self.p.palw_audit_2026_09_23_active_at(daa) => {
                    refunds.push(palw_model_carrier_refund_v1(tx, &carried.object).expect("a market carrier names its payer"));
                }
                Err(_) => {}
            }
        }
        ex.carrier_market_refunds = refunds.clone();
        let (next, delta) = apply_palw_transition_v2_with_extras(&self.state, &self.sp, &ctx, &accepted, None, ua, cb, uw, dc, &ex)
            .expect("the block folds");
        assert_eq!(
            revert_delta_v2(&next, &delta, &self.sp).unwrap().state_root(),
            self.state.state_root(),
            "a reorg undoes the block"
        );
        self.state = next;
        self.daa = daa;
        refunds
    }

    fn owed(&self, carrier: &Transaction) -> Option<(Hash64, u64)> {
        let key = palw_model_refund_payout_key_v1(&carrier.id());
        self.state.pending_payouts_iter().find(|(k, _)| **k == key).map(|(_, row)| (row.payload, row.amount))
    }
}

/// The route matrix's P8, replayed: the market is seeded, B buys at the quote, and C's buy at the
/// SAME quote arrives behind it in the same block and is refused under its own floor. Every carrier
/// ends holding what it paid for or owed exactly what it paid, to the change address it signed.
#[test]
fn a_refused_carrier_buy_is_paid_back_to_its_payer() {
    let mut c = Chain::t12(true);
    let line = *c.state.classes_iter().map(|(id, _)| id).next().expect("testnet-12 registers its models at genesis");
    let (seeder, payer_b, payer_c) = (h(0x5EED), h(0xB0B), h(0xC0C));
    let floor = c.p.palw_model_seed_min_sompi_at(c.daa + 1);
    let seed_tx = carrier(&Obj::ModelSeed { line_id: line, seeder, msk_seed: floor, sink_index: 1 }, seeder, line, floor, 0);
    c.block(std::slice::from_ref(&seed_tx));
    let market = c.state.model_market(&line).copied();
    let opened = market.is_some_and(|m| m.is_open());
    assert!(opened || c.owed(&seed_tx) == Some((seeder, floor)), "the seed opened the pair or is owed back whole");

    let quoted = market
        .filter(|m| m.is_open())
        .and_then(|m| palw_model_buy_quote_with(&m, 1_000 * MSK, PalwModelFeesV1::at(c.p.palw_model_leg_v2_active_at(c.daa + 1))))
        .map(|q| q.units_out)
        .unwrap_or(1);
    let (holder_b, holder_c) = (h(0xB), h(0xC));
    let tx_b = carrier(&buy(line, holder_b, 1_000 * MSK, quoted), payer_b, line, 1_000 * MSK, 1);
    let tx_c = carrier(&buy(line, holder_c, 1_000 * MSK, quoted), payer_c, line, 1_000 * MSK, 2);
    let refunds = c.block(&[tx_b.clone(), tx_c.clone()]);

    println!(
        "[P-B1] line {line}: market open {opened}; B holds {} units; C holds {}; refunds this block {:?}",
        c.state.model_position(&line, &holder_b),
        c.state.model_position(&line, &holder_c),
        refunds.iter().map(|r| (r.payee, r.amount)).collect::<Vec<_>>()
    );
    assert!(!refunds.is_empty(), "C's buy, quoted on the row B's buy already moved, is refused");
    for (tx, holder, payer) in [(&tx_b, holder_b, payer_b), (&tx_c, holder_c, payer_c)] {
        let held = c.state.model_position(&line, &holder);
        let owed = c.owed(tx);
        assert!(
            (held > 0 && owed.is_none()) || (held == 0 && owed == Some((payer, 1_000 * MSK))),
            "the payer {payer} holds {held} units and is owed {owed:?} for a 1,000 MSK carrier"
        );
    }
    assert_eq!(c.state.model_position(&line, &holder_c), 0);
    assert_eq!(c.owed(&tx_c), Some((payer_c, 1_000 * MSK)), "C is paid back what its sink took, at its own change address");
}

/// **The testnet-11 path is the one this change leaves alone.** testnet-11 arms the market and
/// not the 2026-09-23 audit, so its refused carriers keep what they paid in the sink — byte for
/// byte the fold every node on it already runs. The same block with P-B1 unarmed is that fold.
#[test]
fn below_the_audit_fence_a_refused_carrier_is_folded_exactly_as_before() {
    let t11 = network(11);
    assert!(t11.palw_audit_2026_09_23_fence().is_none(), "testnet-11 has not armed the 2026-09-23 audit");
    assert!(!(0..=1_000_000u64).step_by(9_973).any(|daa| t11.palw_audit_2026_09_23_active_at(daa)));

    let mut armed = Chain::t12(true);
    let mut unarmed = Chain::t12(false);
    let line = *armed.state.classes_iter().map(|(id, _)| id).next().unwrap();
    // A buy on a line nobody has seeded: refused on every build, by the same rule.
    let tx = carrier(&buy(line, h(0xD), 10 * MSK, 0), h(0xD0D), line, 10 * MSK, 7);
    assert_eq!(armed.block(std::slice::from_ref(&tx)).len(), 1);
    assert!(unarmed.block(std::slice::from_ref(&tx)).is_empty());
    assert_eq!(armed.owed(&tx), Some((h(0xD0D), 10 * MSK)));
    assert_eq!(unarmed.owed(&tx), None, "unarmed, the payment stays in the sink");
    assert_eq!(
        unarmed.state.pending_payouts_iter().count() + 1,
        armed.state.pending_payouts_iter().count(),
        "and the refund row is the only difference between the two folds"
    );
}

/// The payer is read off the carrier's outputs: the first P2PKH-ML-DSA-87 one, which is the change
/// every shipped tool writes. Not the holder — a buy may be a gift — and never the sink.
#[test]
fn the_refund_goes_to_the_change_the_payer_signed_and_not_to_the_holder() {
    let line = h(0x11);
    let gift = buy(line, h(0x6), 5 * MSK, 0);
    let tx = carrier(&gift, h(0xFA), line, 5 * MSK, 3);
    let refund = palw_model_carrier_refund_v1(&tx, &gift).unwrap();
    assert_eq!((refund.carrier, refund.line_id, refund.payee, refund.amount), (tx.id(), line, h(0xFA), 5 * MSK));

    // A carrier that pays nobody back but the sink has nobody to pay back to.
    let mut bare = tx.clone();
    bare.outputs.remove(0);
    assert_eq!(palw_model_carrier_refund_v1(&bare, &gift), None);
    // Only a market move owes anything.
    assert_eq!(palw_model_carrier_refund_v1(&tx, &Obj::ModelLineRetired { line_id: line, signature: vec![1] }), None);
}

/// A top-up carrier as `misaka palw model-sponsor` builds it: the change back to the payer at output
/// 0, the class's activation sink at output 1, `ActivationPoolFunded` in the payload.
fn top_up_carrier(class: Hash64, payer: Hash64, amount: u64, nonce: u32) -> Transaction {
    let object = Obj::ActivationPoolFunded { class_id: class, amount, sink_index: 1 };
    let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object }).unwrap();
    Transaction::new(
        0,
        vec![TransactionInput::new(TransactionOutpoint::new(h(0xF00D), nonce), vec![], 0, 1)],
        vec![
            TransactionOutput::new(3 * MSK, p2pkh_mldsa87_spk(&payer.as_bytes())),
            TransactionOutput::new(amount, kaspa_consensus_core::palw_activation_pool_v1::palw_activation_sink_spk_v1(&class)),
        ],
        0,
        SUBNETWORK_ID_PALW_LIFECYCLE,
        0,
        payload,
    )
}

/// **ADR-0152-adjacent (Activation Pool): a top-up the fold refuses is paid back through P-B1, one
/// it folds is the pool's** — on testnet-12's own genesis. A top-up of a class the chain does not
/// hold, and one under the least top-up, are refused and owed back whole to the change their payers
/// signed; a top-up of a genesis class opens its pool (all bonus: no Candidate audit can pay it) and
/// is owed nothing. Every sompi a sink took is either in a pool or owed back.
#[test]
fn a_refused_top_up_is_paid_back_and_a_folded_one_is_the_pools() {
    let mut c = Chain::t12(true);
    assert!(c.p.palw_activation_pool_at(1).is_some(), "testnet-12 arms the pool");
    let floor = c.sp.base_class_id();
    let genesis_class =
        *c.state.classes_iter().map(|(id, _)| id).find(|id| **id != floor).expect("testnet-12 registers its models at genesis");
    let (payer_a, payer_b, payer_c, payer_d) = (h(0xA0A), h(0xB0B), h(0xC0C), h(0xD0D));
    let missing = top_up_carrier(h(0xDEAD), payer_a, 50 * MSK, 1);
    let dust = top_up_carrier(genesis_class, payer_b, MSK - 1, 2);
    let kept = top_up_carrier(genesis_class, payer_c, 50 * MSK, 3);
    let on_floor = top_up_carrier(floor, payer_d, 50 * MSK, 4);
    let refunds = c.block(&[missing.clone(), dust.clone(), kept.clone(), on_floor.clone()]);
    println!("[pool] refunds {:?}", refunds.iter().map(|r| (r.payee, r.amount)).collect::<Vec<_>>());
    assert_eq!(c.owed(&missing), Some((payer_a, 50 * MSK)), "a top-up of no class is paid back whole");
    assert_eq!(c.owed(&dust), Some((payer_b, MSK - 1)), "a top-up under the least is paid back whole");
    assert_eq!(c.owed(&on_floor), Some((payer_d, 50 * MSK)), "a top-up of the floor is paid back whole (F3)");
    assert!(c.state.activation_pool(&floor).is_none(), "and the floor has no pool");
    assert_eq!(c.owed(&kept), None, "a folded top-up is a donation");
    let pool = c.state.activation_pool(&genesis_class).cloned().expect("the genesis class's pool opened");
    assert_eq!((pool.funded_sompi, pool.bonus_sompi, pool.prep_sompi), (50 * MSK, 50 * MSK, 0));
}

/// **testnet-11 does not arm the pool: a top-up carrier there is refused as dormant** — and, with
/// no P-B1 below the audit fence, keeps what it paid, exactly as any refused market carrier does
/// there. (On testnet-11 no activation sink is a legal output at all: its validator refuses the
/// form at isolation, so this block can only be a rehearsal.)
#[test]
fn on_testnet_11_a_top_up_folds_nothing() {
    let p = network(11);
    assert!(p.palw_activation_pool_at(u64::MAX - 1).is_none());
    assert!(extras(&p, 1).activation_pool.is_none());
}
