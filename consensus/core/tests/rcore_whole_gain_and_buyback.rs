//! **ADR-0152 R-core+ after the S review: what a seat's lock is priced on** — H1 (the whole gain while
//! no vesting row exists), M2 (the buyback bound `s` in `G_res`) and L1 (the processor's extras: the
//! execution lane's quantum counts the execution rights `R`), on testnet-12's own fold.
//!
//! H1. L-1 prices the lock on the residual `G_res = G − E` because a Final's escrow is meant to sit in a
//! vesting row a conviction burns. This build has no such row — `finalize_claim` pays `E` and
//! `burn_vesting_row` is a stub — so `PALW_RCORE_VESTING_ROWS_LANDED_V1` is `false` and every lock is
//! priced on the whole `G`: the smallest licensing set's locks, which are all a post-Final conviction
//! can take, out-value it with the margin. The residual prices are pinned beside it through
//! `palw_rcore_lock_vested_v1`, which the vesting work arms by flipping the flag.
//!
//! M2. Where the claim's line has an open pair, a Final buys `s` (5% of `E`) from it; `s` never vests,
//! so the residual gain carries it: `G_res = w + R + s` and the escrow term is on `E − s`. Pricing
//! `s = 0` made the residual lock cheaper by about `s / k`. The S re-review: a pair can open between
//! the licence and the Final, so the price the vesting build arms (`palw_rcore_lock_vested_at_cap_v1`)
//! takes `s` at its cap, `5% · E`, whatever the pair's state at the licence.
//!
//! Run: cargo test -p kaspa-consensus-core --test rcore_whole_gain_and_buyback

#[path = "rcore_common.rs"]
mod common;
use common::*;

use kaspa_consensus_core::palw_model_lines_v1::PalwArtifactOwnerV1;
use kaspa_consensus_core::palw_model_market_v1::{PalwModelMarketV1, palw_model_buyback_slice_v1};
use kaspa_consensus_core::palw_state_v2::{
    PALW_RCORE_VESTING_ROWS_LANDED_V1, palw_rcore_bind_prices_v1, palw_rcore_lock_vested_at_cap_v1, palw_rcore_lock_vested_v1,
    palw_rcore_seat_lock_v1,
};

const MSK: u128 = 100_000_000;

fn msk_of(sompi: u128) -> f64 {
    sompi as f64 / MSK as f64
}

/// A claim of `class` (`None`: the floor) on its own chain, with the panel it binds.
fn claim_of(class: Option<Hash64>, seed: u64) -> (Chain, Hash64, Vec<(PalwBondKeyV2, Hash64)>) {
    // ADR-0152 §4-quater (U-D1): the 2M row is closed at launch, so its claim exists only past the flag
    // day that installs its measured row (`t12_2m_open`); the floor and 8k rows read the same.
    let p = t12_2m_open();
    match class {
        None => {
            let mut c = Chain::new(p);
            let id = c.floor_claim(seed);
            let seats = c.floor_seats();
            (c, id, seats)
        }
        Some(class) => {
            let mut c = model_chain(p.clone(), class, 1);
            let id = model_claim(&mut c, class, 1, seed);
            c.s = readied(&c.sp, &c.s, &honest(&c.p), class, c.daa);
            (c, id, honest_seats(&p, 5))
        }
    }
}

/// **H1: three `lock_3` and two `lock_2` out-value the whole gain on every class, and after the Final
/// has paid `E` the three quorum signers still hold that much.** Floor 1,173.69 MSK per `lock_3`. The residual prices (the vesting build's) are ADR §2's, from the same
/// `G_res`: floor 106.74 / 160.11, 8k 306.08 / 459.12, 2M 22,252.74 / 33,379.11.
#[test]
fn h1_the_lock_is_priced_on_the_whole_gain_until_vesting_rows_land() {
    assert!(!PALW_RCORE_VESTING_ROWS_LANDED_V1, "the premise: finalize_claim pays E and no row can burn it");
    // ADR-0152 §4-quater (U-D1): the 2M row is closed at launch, so a live 2M claim exists only past the flag day
    // that installs its measured row — this test's premise runs there (`t12_2m_open`, measuring the derived
    // 13,995-DAA deadline).
    let p = t12_2m_open();
    let (short, id2m) = model_classes(&p);
    let near = |got: u128, want: f64| (msk_of(got) - want).abs() < 0.02;
    for (name, class, residual) in
        [("floor", None, (106.74, 160.11)), ("8k", Some(short), (306.08, 459.12)), ("2M", Some(id2m), (22_252.74, 33_379.11))]
    {
        let (mut c, id, seats) = claim_of(class, 0x4101);
        let claim = c.claim(&id);
        let e = c.extras_at(c.daa + 1);
        let prices = palw_rcore_bind_prices_v1(&c.s, &c.sp, &e, &id, &claim, 5, c.daa + 1);
        let gain = prices.g_res + u128::from(claim.escrowed_reward);
        let lock_3 = palw_rcore_seat_lock_v1(&c.s, &c.sp, &e, &id, &claim, 3);
        assert_eq!(prices.lock_2, palw_rcore_seat_lock_v1(&c.s, &c.sp, &e, &id, &claim, 2));
        assert!(3 * lock_3 > gain + gain / 10 - 3, "{name}: three lock_3 out-value G with the margin");
        assert!(2 * prices.lock_2 > gain + gain / 10 - 2, "{name}: two lock_2 out-value G with the margin");
        let (r3, r2) = (
            palw_rcore_lock_vested_v1(prices.g_res, claim.escrowed_reward, 0, 3),
            palw_rcore_lock_vested_v1(prices.g_res, claim.escrowed_reward, 0, 2),
        );
        assert!(
            near(r3, residual.0) && near(r2, residual.1),
            "{name}: residual lock_3 / lock_2 {:.4} / {:.4} MSK (ADR {} / {})",
            msk_of(r3),
            msk_of(r2),
            residual.0,
            residual.1
        );
        // The hazard the flag closes: where `E` dominates the gain (the floor, 8k) the residual price
        // alone leaves a post-Final conviction short of `G`; on 2M `w` dominates and it would not.
        assert_eq!(3 * r3 < gain, name != "2M", "{name}: whether the residual price alone falls short of G");
        if name == "floor" {
            assert!(near(lock_3, 1_173.69), "floor lock_3 on G {:.4} MSK", msk_of(lock_3));
            // Through the licence and the Final: E is paid, and the quorum's three live locks still
            // out-value G — the whole recovery a post-Final conviction has.
            let bound = c.bind(id, &seats);
            c.step(&[PalwConsensusObjectV2::ReceiptLicensed {
                claim: id,
                receipts: seats[..3].iter().map(|(k, _)| valid(id, *k, bound)).collect(),
            }]);
            c.finalize(id);
            let held: u128 = seats[..3]
                .iter()
                .map(|(k, _)| c.s.slashable_lock(*k, id).expect("each signer's lock outlives the Final").amount)
                .sum();
            assert_eq!(held, 3 * lock_3, "the three quorum locks");
            assert!(held > gain, "after the Final paid E, the locks still out-value G");
        }
        println!(
            "H1 {name}: G {:.4} MSK (G_res {:.4}, E {:.4}); lock_3 {:.4} lock_2 {:.4} on G; residual {:.4} / {:.4}",
            msk_of(gain),
            msk_of(prices.g_res),
            msk_of(u128::from(claim.escrowed_reward)),
            msk_of(lock_3),
            msk_of(prices.lock_2),
            msk_of(r3),
            msk_of(r2)
        );
    }
}

/// **M2: an open pair on the claim's line puts the buyback bound in `G_res`.** An 8k claim whose root
/// is owned by a line with a seeded market: `G_res` rises by exactly `s = 5% · E` (≈ 160 MSK), and the
/// residual `lock_3` — `palw_seat_lock_required_v2(G_res, 3) + ⌈100‰ · (E − s) / 3⌉` — is about
/// `s / 3` (≈ 53 MSK) above the `s = 0` price the deviation used (`s` out of both terms). With no market, `s = 0`. The fold's
/// lock (on `G`, H1) does not move with `s`: the slice is inside `E`.
#[test]
fn m2_an_open_pair_puts_the_buyback_bound_in_the_residual_gain() {
    let p = t12();
    let (short, _) = model_classes(&p);
    let (c, id, _) = claim_of(Some(short), 0x4201);
    let claim = c.claim(&id);
    let e = c.extras_at(c.daa + 1);
    // The genesis 8k row has no line, so nothing records the claim's root; the carriage writes what a
    // line whose version carries that root would have: the claim's root, the root's owner row, and a
    // seeded pair on the line.
    assert_eq!(c.s.claim_root(&id), None, "the premise: no line, no root, s = 0 at launch");
    let root = c.s.class(&short).expect("the 8k row").artifact_root;
    let line = h(0x11_AE01);
    let at = c.daa;
    let open = edited(&c.sp, &c.s, |k| {
        k.claim_roots.insert(id, root);
        k.artifact_owners.insert((short, root), PalwArtifactOwnerV1 { class_id: short, line_id: line, version: 1 });
        k.model_markets.insert(line, PalwModelMarketV1::seed_v1(at, 100_000 * MSK as u64, h(0x11_AE02)));
    });
    assert!(e.artifact_root_ownership_active, "the premise: testnet-12 attributes a root through its owner row");
    let without = palw_rcore_bind_prices_v1(&c.s, &c.sp, &e, &id, &claim, 5, c.daa + 1);
    let with = palw_rcore_bind_prices_v1(&open, &c.sp, &e, &id, &claim, 5, c.daa + 1);
    let s = palw_model_buyback_slice_v1(claim.escrowed_reward);
    assert!((msk_of(u128::from(s)) - 160.0).abs() < 1.0, "s = 5% of the 8k escrow: {:.4} MSK", msk_of(u128::from(s)));
    assert_eq!(with.g_res, without.g_res + u128::from(s), "G_res carries s where the pair is open");
    let residual_with = palw_rcore_lock_vested_v1(with.g_res, claim.escrowed_reward, s, 3);
    // The deviation: `s = 0` in both places — out of `G_res` and out of the escrow term's `E − s`.
    let residual_s0 = palw_rcore_lock_vested_v1(without.g_res, claim.escrowed_reward, 0, 3);
    let dropped = residual_with - residual_s0;
    assert!(
        (msk_of(dropped) - msk_of(u128::from(s)) / 3.0).abs() < 0.5,
        "s = 0 would drop lock_3 by ≈ 1.1·s/3 − 0.1·s/3 = s/3: {:.4} MSK",
        msk_of(dropped)
    );
    assert!(dropped > 50 * MSK, "the deviation's discount was ≈ 53 MSK");
    // The S re-review: the price the vesting build arms takes `s` at its cap whatever the pair's state
    // — the pair open at the licence and a pair that opens only before the Final are priced alike.
    let armed_open = palw_rcore_lock_vested_at_cap_v1(with.g_res, claim.escrowed_reward, s, 3);
    let armed_closed = palw_rcore_lock_vested_at_cap_v1(without.g_res, claim.escrowed_reward, 0, 3);
    assert_eq!(armed_closed, armed_open, "the armed price does not depend on the pair's state at the licence");
    assert_eq!(armed_open, residual_with, "it is the open pair's price: s = 5% · E is the cap");
    assert!(armed_closed > residual_s0, "a closed pair at the licence no longer discounts the lock");
    assert_eq!(
        palw_rcore_seat_lock_v1(&open, &c.sp, &e, &id, &claim, 3),
        palw_rcore_seat_lock_v1(&c.s, &c.sp, &e, &id, &claim, 3),
        "the whole-gain lock does not move with s: the slice is part of E"
    );
    // A closed pair buys nothing: s = 0.
    let closed = edited(&c.sp, &open, |k| k.model_markets.get_mut(&line).unwrap().closed_to_buys = true);
    assert_eq!(palw_rcore_bind_prices_v1(&closed, &c.sp, &e, &id, &claim, 5, c.daa + 1).g_res, without.g_res, "closed: s = 0");
    println!(
        "M2 8k: s {:.4} MSK; G_res {:.4} → {:.4}; residual lock_3 {:.4} (s = 0 would be {:.4}, −{:.4})",
        msk_of(u128::from(s)),
        msk_of(without.g_res),
        msk_of(with.g_res),
        msk_of(residual_with),
        msk_of(residual_s0),
        msk_of(dropped)
    );
}
