//! MSK-26A-PALW-12 — model-market payout rows are keyed by (line, holder, DAA, per-block move
//! counter, leg) and written by overwrite, so when two consecutive chain blocks share a DAA score a
//! later move silently replaces an earlier move's still-queued payout row.
//!
//! Audit commit: 3d2bd6dc5d77d37396d1b73c4c13526090923b92
//! Crate:        kaspa-consensus-core (consensus/core)
//! Command:
//!   cp docs/security/audits/2026-09-claude-pre-freeze/poc/MSK-26A-PALW-12.rs \
//!      consensus/core/tests/audit_poc_msk_26a_palw_12.rs && \
//!   cargo test -p kaspa-consensus-core --test audit_poc_msk_26a_palw_12 -- --nocapture ; \
//!   rm consensus/core/tests/audit_poc_msk_26a_palw_12.rs
//!
//! PASS = the vulnerable behaviour is present: on `palw_t12_shipped_params()` (testnet-12 as shipped)
//! a queued `pending_payouts` row written by a market move in chain block N is REPLACED (delta entry
//! `Payout { old: Some(first), new: Some(second) }`, no drain in between) by a market move in chain
//! block N+1 at the same DAA score, and the first row's sompi are never drained/paid by any later
//! coinbase, while the market's `registrant_paid_sompi` still counts them. The control run (N+1 one
//! DAA later) pays both rows, so equal DAA is the root cause. The tests FAIL once the key is made
//! unique per chain block (or the write accumulates).
//!
//! Scope of the PoC: the fold (`apply_palw_transition_v2_with_extras` -> `apply_palw_transition_v7`),
//! driven exactly as the processor drives it: one `palw_v2_apply_one_object_v1` rehearsal per object
//! (every object here is one the processor would keep) and then the chain block's transition with
//! `ctx.daa_score = header.daa_score`. The ML-DSA-87 sell signature and the `not_after_daa` window
//! are checked by the processor's acceptance layer (processor.rs:8671-8715), not by the fold; in the
//! honest-loss scenario both sells are signed by the holder itself, so that layer is satisfied by
//! construction and a placeholder signature is used here. Equal DAA across consecutive chain blocks
//! is consensus behaviour (difficulty.rs:44-56 + palw_lane_advances_daa_v1: past `palw_anchor_clock`
//! a lane `bits` does not price — attempt lanes 6/9 past `palw_single_lottery`, receipt 7,
//! heartbeat 8 — does not advance the score); the fold explicitly accepts it (palw_state_v2.rs:22589).

use kaspa_consensus_core::config::params::{ForkActivation, Params, mainnet_shipped_params, palw_t12_shipped_params};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_model_market_v1::{palw_model_holder_of_pubkey_v1, palw_model_sell_net_payload_v1};
use kaspa_consensus_core::palw_state_v2::{
    PALW_STATE_V2_MODEL_PAYOUT_KEY_PREFIX, PALW_V2_MAX_PAYOUTS_PER_BLOCK, PalwBlockContextV2, PalwChainStateV2,
    PalwConsensusObjectV2 as Obj, PalwDeltaEntryV2, PalwPayoutV2, PalwStateDeltaV2, PalwStateParamsV2, PalwTransitionExtrasV1,
    apply_palw_transition_v2_with_extras, palw_v2_apply_one_object_v1, palw_v2_pre_object_base_v1,
};
use kaspa_consensus_core::pow_layer0::{
    POW_ALGO_ID_HEARTBEAT_V1, POW_ALGO_ID_KHEAVYHASH, POW_ALGO_ID_PALW_COMMITTED_V2, POW_ALGO_ID_PALW_EXEC_V3,
    POW_ALGO_ID_PALW_RECEIPT_V3, algo_id_is_priced_by_bits_v3,
};
use kaspa_hashes::Hash64;
use std::collections::BTreeMap;

const MSK: u64 = 100_000_000;

fn h(v: u64) -> Hash64 {
    Hash64::from_u64_word(v)
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
        activation_pool: p.palw_activation_pool_at(daa),
        ..Default::default()
    }
}

/// A seller: an ML-DSA-87-sized public key, its holder id (the position key) and the payload its
/// carrier sell's net leg is paid to past the 2026-09-23 fence (P-B4).
struct Seller {
    pubkey: Vec<u8>,
    holder: Hash64,
    net_payload: Hash64,
}

fn seller(tag: u8) -> Seller {
    let pubkey = vec![tag; 2592];
    Seller { holder: palw_model_holder_of_pubkey_v1(&pubkey), net_payload: palw_model_sell_net_payload_v1(&pubkey), pubkey }
}

fn buy(line: Hash64, holder: Hash64, msk_in: u64) -> Obj {
    Obj::ModelBuy { line_id: line, holder, msk_in, min_units_out: 0, sink_index: 1 }
}

fn sell(line: Hash64, s: &Seller, units_in: u64, held_units: u64, daa: u64) -> Obj {
    Obj::ModelSell {
        line_id: line,
        holder: s.holder,
        units_in,
        min_msk_out: 0,
        held_units,
        not_after_daa: daa + 10,
        pubkey: s.pubkey.clone(),
        // Verified by the processor's acceptance layer, never by the fold (see the header).
        signature: vec![1u8; 4627],
    }
}

struct Chain {
    p: Params,
    sp: PalwStateParamsV2,
    state: PalwChainStateV2,
    blue: u64,
    daa: u64,
    /// Every row the drain removed, i.e. every row some coinbase paid: payload -> sompi.
    paid: BTreeMap<Hash64, u64>,
}

impl Chain {
    fn t12() -> Self {
        let p = palw_t12_shipped_params();
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
        Chain { p, sp, state, blue: 0, daa: 0, paid: BTreeMap::new() }
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

    /// One selected-chain block at DAA `daa` (blue score strictly increasing, as along any chain),
    /// accepting `objects`. Every object must pass the processor's per-object rehearsal.
    fn block(&mut self, daa: u64, objects: &[Obj]) -> PalwStateDeltaV2 {
        let blue = self.blue + 1;
        let ctx = PalwBlockContextV2 { block: h(0x1000_0000 | blue), daa_score: daa, blue_score: blue, subsidy: 0 };
        let (ua, cb, uw, dc) = self.flags(daa);
        let ex = extras(&self.p, daa);
        let mut folded = palw_v2_pre_object_base_v1(&self.state, &self.sp, &ctx, ua, cb, uw, dc, &ex).expect("pre-object base");
        for object in objects {
            folded = palw_v2_apply_one_object_v1(&folded, &self.sp, &ctx, object, ua, cb, uw, dc, &ex)
                .unwrap_or_else(|e| panic!("the rehearsal keeps every object of this block: {e:?}"));
        }
        let (next, delta) =
            apply_palw_transition_v2_with_extras(&self.state, &self.sp, &ctx, objects, None, ua, cb, uw, dc, &ex).expect("the block folds");
        // A row the block REMOVED is a row this block's coinbase paid (step 1b, palw_state_v2.rs:22621).
        for entry in &delta.entries {
            if let PalwDeltaEntryV2::Payout { old: Some(row), new: None, .. } = entry {
                *self.paid.entry(row.payload).or_default() += row.amount;
            }
        }
        self.state = next;
        self.blue = blue;
        self.daa = daa;
        delta
    }

    fn drain_everything(&mut self) {
        let mut guard = 0;
        while self.state.pending_payouts_iter().next().is_some() {
            let daa = self.daa + 1;
            self.block(daa, &[]);
            guard += 1;
            assert!(guard < 500, "the queue drains");
        }
    }

    fn rows_to(&self, payload: &Hash64) -> Vec<(Hash64, u64)> {
        self.state.pending_payouts_iter().filter(|(_, r)| r.payload == *payload).map(|(k, r)| (*k, r.amount)).collect()
    }

    /// The keys the NEXT block's drain takes (the first `PALW_V2_MAX_PAYOUTS_PER_BLOCK` in key order).
    fn next_drain(&self) -> Vec<Hash64> {
        self.state.pending_payouts_iter().map(|(k, _)| *k).take(PALW_V2_MAX_PAYOUTS_PER_BLOCK).collect()
    }

    fn paid_to(&self, payload: &Hash64) -> u64 {
        self.paid.get(payload).copied().unwrap_or(0)
    }
}

fn overwrites_in(delta: &PalwStateDeltaV2, key: &Hash64) -> Vec<(Option<PalwPayoutV2>, Option<PalwPayoutV2>)> {
    delta
        .entries
        .iter()
        .filter_map(|e| match e {
            PalwDeltaEntryV2::Payout { key: k, old, new } if k == key => Some((old.clone(), new.clone())),
            _ => None,
        })
        .collect()
}

/// Seed the genesis line and give the holders positions. Returns the line.
fn open_market(c: &mut Chain, holders: &[Hash64], each_msk: u64) -> Hash64 {
    let line = *c.state.classes_iter().map(|(id, _)| id).next().expect("testnet-12 registers its models at genesis");
    seed_and_buy(c, line, holders, each_msk);
    line
}

/// **A line whose owner is a bonded party** (ADR-0088 Decision 1: any Active bond founds a line on
/// an Active class and becomes its owner), seeded, with the holders' positions. On testnet-12's
/// genesis the founding lines' owner legs are burned (their registrant has no bond row), so the
/// owner-leg wipe needs a founded line — the normal way a line gets an owner who is paid.
fn open_owned_market(c: &mut Chain, holders: &[Hash64], each_msk: u64) -> (Hash64, Hash64) {
    use kaspa_consensus_core::palw_model_lines_v1::model_line_id_v1;
    use kaspa_consensus_core::palw_state_v2::PalwBondStatusV2;
    let base = c.sp.base_class_id();
    let class_id = *c
        .state
        .classes_iter()
        .find(|(id, cls)| **id != base && matches!(cls.status, kaspa_consensus_core::palw_state_v2::PalwClassStatusV2::Active))
        .map(|(id, _)| id)
        .expect("an Active non-floor genesis class");
    let (founder, owner_payload) = c
        .state
        .bonds_iter()
        .find(|(_, b)| matches!(b.status, PalwBondStatusV2::Active))
        .map(|(k, b)| (*k, b.payout_payload))
        .expect("an Active genesis bond");
    let name = b"poc-line".to_vec();
    let line = model_line_id_v1(&class_id, &founder, &name);
    let d = c.daa + 1;
    c.block(d, &[Obj::ModelLineFounded { class_id, name, founder, root: h(0xB0B0_B0B0), signature: vec![1] }]);
    seed_and_buy(c, line, holders, each_msk);
    (line, owner_payload)
}

fn seed_and_buy(c: &mut Chain, line: Hash64, holders: &[Hash64], each_msk: u64) {
    let d = c.daa + 1;
    let floor = c.p.palw_model_seed_min_sompi_at(d);
    c.block(d, &[Obj::ModelSeed { line_id: line, seeder: h(0x5EED), msk_seed: floor, sink_index: 1 }]);
    assert!(c.state.model_market(&line).is_some_and(|m| m.is_open()), "the seed opened the pair");
    let buys: Vec<Obj> = holders.iter().map(|holder| buy(line, *holder, each_msk)).collect();
    c.block(d + 1, &buys);
    for holder in holders {
        assert!(c.state.model_position(&line, holder) > 0, "every holder holds units");
    }
    // Start from an empty queue so every sompi below is accounted for.
    c.drain_everything();
}

/// The shipped fences this depends on (reachability, printed and asserted).
#[test]
fn msk_26a_palw_12_fences_on_the_shipped_constructors() {
    let t12 = palw_t12_shipped_params();
    let main = mainnet_shipped_params();
    for (name, p) in [("testnet-12", &t12), ("mainnet", &main)] {
        println!(
            "[{name}] model_market {:?} model_lines {:?} model_leg_v2 {:?} audit_2026_09_23 {:?} anchor_clock {:?} single_lottery {:?} clock_cursor {:?}",
            p.palw_model_market, p.palw_model_lines, p.palw_model_leg_v2, p.palw_audit_2026_09_23, p.palw_anchor_clock,
            p.palw_single_lottery, p.palw_clock_cursor
        );
    }
    assert!(t12.palw_model_market_active_at(0) && t12.palw_model_lines_active_at(0) && t12.palw_audit_2026_09_23_active_at(0));
    assert!(t12.palw_anchor_clock.is_some_and(|f| f.is_active(0)), "testnet-12 runs the anchor clock from genesis");
    assert!(t12.palw_single_lottery.is_some_and(|f| f.is_active(0)), "and the single lottery");
    // Past the single lottery an attempt block (6/9), a receipt (7) and a heartbeat (8) are not priced
    // by bits, so (difficulty.rs palw_lane_advances_daa_v1) they do not advance the DAA score: a chain
    // block whose mergeset is only such blocks has its selected parent's DAA score.
    for lane in [POW_ALGO_ID_PALW_COMMITTED_V2, POW_ALGO_ID_PALW_EXEC_V3, POW_ALGO_ID_PALW_RECEIPT_V3, POW_ALGO_ID_HEARTBEAT_V1] {
        assert!(!algo_id_is_priced_by_bits_v3(lane), "lane {lane} does not advance the DAA score");
    }
    assert!(algo_id_is_priced_by_bits_v3(POW_ALGO_ID_KHEAVYHASH));
    assert!(main.palw_model_market.is_none(), "the mainnet preset does not arm the market");
}

/// **Honest loss.** Holder H sells twice from one position: the first sell is the first market move
/// of chain block N (DAA d), the second, signed against the new position, the first market move of
/// chain block N+1, also at DAA d. The second sell's rows are written over the first's.
#[test]
fn msk_26a_palw_12_second_sell_at_equal_daa_overwrites_the_first_sells_queued_proceeds() {
    let run = |equal_daa: bool| {
        let mut c = Chain::t12();
        let h_seller = seller(0xA1);
        let backlog: Vec<Seller> = (0..24u8).map(|i| seller(0x10 + i)).collect();
        let mut holders = vec![h_seller.holder];
        holders.extend(backlog.iter().map(|s| s.holder));
        let line = open_market(&mut c, &holders, 20_000 * MSK);
        let owner_paid_before = c.state.model_market(&line).unwrap().registrant_paid_sompi;

        // Chain block N at DAA d: H's first sell is market move 0; other holders' sells behind it
        // are ordinary traffic that leaves a queue deeper than one block's drain.
        let d = c.daa + 1;
        let held0 = c.state.model_position(&line, &h_seller.holder);
        let units1 = held0 / 3;
        let mut objects = vec![sell(line, &h_seller, units1, held0, d)];
        for s in &backlog {
            let held = c.state.model_position(&line, &s.holder);
            objects.push(sell(line, s, held / 2, held, d));
        }
        c.block(d, &objects);
        let rows1 = c.rows_to(&h_seller.net_payload);
        assert_eq!(rows1.len(), 1, "the first sell queued one net row for H");
        let (r1_key, net1) = rows1[0];
        assert_eq!(r1_key.as_bytes()[0], PALW_STATE_V2_MODEL_PAYOUT_KEY_PREFIX);
        assert!(!c.next_drain().contains(&r1_key), "R1 is still queued behind the backlog at N+1's start");

        // Chain block N+1: the same DAA score (an unpriced selected parent), or d+1 for the control.
        let d2 = if equal_daa { d } else { d + 1 };
        let held1 = c.state.model_position(&line, &h_seller.holder);
        assert_eq!(held1, held0 - units1);
        let units2 = held1 / 2;
        let delta = c.block(d2, &[sell(line, &h_seller, units2, held1, d2)]);
        let r1_writes = overwrites_in(&delta, &r1_key);
        let rows2 = c.rows_to(&h_seller.net_payload);
        let owner_paid_after = c.state.model_market(&line).unwrap().registrant_paid_sompi;

        c.drain_everything();
        let paid_h = c.paid_to(&h_seller.net_payload);
        (net1, r1_writes, rows2, paid_h, owner_paid_after - owner_paid_before)
    };

    // ---- equal DAA (testnet-12 as shipped) ----
    let (net1, r1_writes, rows2, paid_h, _) = run(true);
    println!("[equal DAA] first sell net {net1} sompi; writes to R1 in block N+1: {r1_writes:?}");
    println!("[equal DAA] H's net rows after N+1: {rows2:?}; total ever paid to H after draining: {paid_h}");
    assert_eq!(r1_writes.len(), 1, "block N+1 writes R1 exactly once, and it is not a drain");
    let (old, new) = &r1_writes[0];
    assert_eq!(old.as_ref().map(|r| r.amount), Some(net1), "the write REPLACES the first sale's queued row");
    let net2 = new.as_ref().map(|r| r.amount).expect("with the second sale's row");
    assert_eq!(rows2.len(), 1, "one row for two sales");
    assert_eq!(rows2[0].1, net2, "holding the second sale's net only (not net1 + net2)");
    assert_eq!(paid_h, net2, "H is paid for the second sale only");
    assert!(paid_h < net1 + net2, "the first sale's {net1} sompi are never paid");

    // ---- control: N+1 one DAA later ----
    let (c_net1, c_writes, c_rows2, c_paid_h, _) = run(false);
    println!("[control, DAA d+1] writes to R1 in N+1: {c_writes:?}; H's rows {c_rows2:?}; paid to H {c_paid_h}");
    assert!(c_writes.is_empty(), "a distinct DAA mints a distinct key");
    assert_eq!(c_rows2.len(), 2, "two sales, two rows");
    assert!(c_rows2.iter().any(|(_, a)| *a == c_net1), "the first sale's row is still there");
    assert_eq!(c_paid_h, c_rows2.iter().map(|(_, a)| *a).sum::<u64>(), "both sales paid");
    println!("[result] equal DAA: H paid {paid_h} for sales worth {} (lost {net1}); control paid {c_paid_h}", net1 + net2);
}

/// **Third-party owner-leg wipe.** Victim V buys (market move 0 of chain block N), queuing the line
/// owner's leg. In chain block N+1 at the same DAA an attacker's tiny ModelBuy naming `holder = V`
/// (the holder field is authorised only by the sink payment) is market move 0 and writes the same
/// "buy-owner" key, replacing the owner's row.
///
/// Whether V's row is still queued at N+1's start depends on where its hash sorts among the backlog
/// (the drain takes the first 8 in key order), so the scenario is run for 16 victims: every victim
/// whose row survives the drain loses the owner's leg; a victim whose row is drained at N+1 loses
/// nothing (the row was paid). Both counts are printed.
fn gift_scenario(victim: Hash64) -> Option<(u64, u64, u64, u64)> {
    let mut c = Chain::t12();
    let backlog: Vec<Seller> = (0..24u8).map(|i| seller(0x40 + i)).collect();
    let holders: Vec<Hash64> = backlog.iter().map(|s| s.holder).collect();
    let (line, owner_payload) = open_owned_market(&mut c, &holders, 20_000 * MSK);
    let owner_paid_before = c.state.model_market(&line).unwrap().registrant_paid_sompi;
    let owner_drained_before = c.paid_to(&owner_payload);
    assert!(owner_paid_before > 0, "the founded line pays its bonded owner a leg");
    assert!(c.state.pending_payouts_iter().next().is_none(), "the queue starts empty");

    let d = c.daa + 1;
    let mut objects = vec![buy(line, victim, 10_000 * MSK)];
    for s in &backlog {
        let held = c.state.model_position(&line, &s.holder);
        objects.push(sell(line, s, held / 2, held, d));
    }
    let delta_n = c.block(d, &objects);
    // The victim's buy is object 0: the block's first Payout insert is its owner leg.
    let (o1_key, o1) = delta_n
        .entries
        .iter()
        .find_map(|e| match e {
            PalwDeltaEntryV2::Payout { key, old: None, new: Some(row) } => Some((*key, row.clone())),
            _ => None,
        })
        .expect("the victim's buy queued the owner's leg");
    assert_eq!(o1.payload, owner_payload, "the first row of block N is the victim buy's owner leg");
    assert!(o1.amount > 0);
    let survives = !c.next_drain().contains(&o1_key);

    // Chain block N+1 at the same DAA: the attacker's 5 MSK gift buy for V is market move 0.
    let owner_before_attack = c.state.model_market(&line).unwrap().registrant_paid_sompi;
    let delta = c.block(d, &[buy(line, victim, 5 * MSK)]);
    let writes = overwrites_in(&delta, &o1_key);
    let owner_leg_2 = c.state.model_market(&line).unwrap().registrant_paid_sompi - owner_before_attack;
    let owner_credited = c.state.model_market(&line).unwrap().registrant_paid_sompi - owner_paid_before;
    c.drain_everything();
    let owner_paid = c.paid_to(&owner_payload) - owner_drained_before;
    if !survives {
        // Drained (paid) at N+1's start, then re-inserted: nothing lost.
        assert_eq!(owner_paid, owner_credited, "a drained row was paid; no loss for this victim");
        return None;
    }
    println!(
        "[gift] victim {victim:.16}…: 10,000 MSK buy -> owner row {} sompi; attacker 5 MSK buy for V at the same DAA -> write {writes:?}",
        o1.amount
    );
    assert_eq!(writes.len(), 1, "one write to the owner's key in N+1, and it is not a drain");
    assert_eq!(writes[0].0.as_ref().map(|r| r.amount), Some(o1.amount), "the owner's queued row is replaced");
    assert_eq!(writes[0].1.as_ref().map(|r| (r.payload, r.amount)), Some((owner_payload, owner_leg_2)), "by the gift's tiny leg");
    assert!(owner_leg_2 < o1.amount / 100, "a 5 MSK buy wipes a 10,000 MSK buy's owner leg");
    assert_eq!(owner_paid + o1.amount, owner_credited, "registrant_paid_sompi counts sompi no coinbase ever pays");
    Some((o1.amount, owner_leg_2, owner_credited, owner_paid))
}

#[test]
fn msk_26a_palw_12_gift_buy_at_equal_daa_wipes_the_owners_queued_leg() {
    let mut wiped = 0;
    let mut example = None;
    for i in 0..16u64 {
        if let Some(r) = gift_scenario(h(0x71C7_0000 + i)) {
            wiped += 1;
            example.get_or_insert(r);
        }
    }
    let (lost, gift_leg, credited, paid) = example.expect("at least one victim's owner leg is wiped");
    println!(
        "[gift] owner leg wiped for {wiped}/16 victims (the rest were drained before N+1's write). Example: owner row {lost} sompi \
         replaced by {gift_leg}; market credits the owner {credited}, coinbases ever pay {paid}"
    );
    assert!(wiped > 0);
}
