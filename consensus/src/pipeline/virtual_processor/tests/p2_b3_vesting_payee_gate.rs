//! **ADR-0152 Phase 2, P2-1 — B-3's vesting term at the processor's UTXO sites, on testnet-12**
//! (T23's processor half and T05's bond half; phase2-plan §1.2, F7).
//!
//! B-3 holds a bond's collateral while the bond is the payee of a vesting row still unmatured by
//! V-4(a) — the lock predicate with the liveness escape, and nothing else: NOT the licence halt
//! (V-4(b)), NOT a row that has latched and only waits for V-7's budget (a latched row is mature).
//! The processor reaches that predicate, `palw_bond_collateral_is_locked_v6`, from four places, and
//! every one must answer with it:
//!
//! * the block path's locked set, `palw_v2_locked_bond_outpoints` (the chain walk's
//!   `ctx.palw_v2_locked_bonds`), and the per-transaction check it feeds,
//!   `validate_transaction_in_utxo_context`'s `SpendsNonReleasableBond`;
//! * the burn obligations beside it, `palw_v2_bond_burn_obligations`, the same predicate negated
//!   (a released bond that lost collateral owes `BondBurnNotPaid`);
//! * the mempool's bond gate (`palw_mempool_bond_gate`, memoised per tip and DAA: the locked set
//!   and, since this suite's review, the burn obligations);
//! * the wallet's set, `palw_locked_bond_outpoints_v2_impl` (the RPC's `locked_bond_outpoints`,
//!   which is how the hold reaches `misaka wallet` with no wallet change, F7).
//!
//! **The scenario.** Cards 3 and 4 have retired long enough ago that their withdrawal delay and the
//! retirement's own second clock have both run, and each lost `SLASHED` sompi — so the ONLY thing
//! that can still hold either collateral is the vesting row naming them, card 3 as its producer and
//! card 4 as a credited seat (the payee index's seat entries, not only its producer entries). The
//! row is `Final` at `F`, its DAA clock runs out at `E = F + window_court` (3,000 on testnet-12),
//! and its second clock needs `palw_settled_anchor_depth` (30) anchors after `F`. The cases are the
//! ADR's and their edges:
//!
//! | case | DAA | held |
//! |---|---|---|
//! | `F + 2,999` | `E − 1` | yes — the DAA clock |
//! | a licence halt before `E` | `E − 1`, empty ring | yes — V-4(a)'s DAA clock; the halt only drops the second clock |
//! | `F + 3,000`, the licences in | `E` | no |
//! | `F + 3,000`, one licence short | `E` | yes — the second clock |
//! | `F + 3,001`, one licence short | `E + 1` | yes |
//! | a licence halt past `E` | `E + 2`, empty ring | no — B-3 does not hold on the halt |
//! | a carried row | `E + 3`, latched | no — a latched row is mature |
//! | one DAA inside the per-obligation bound | `E + 2w − 1` | yes |
//! | the per-obligation bound | `E + 2w` | no |
//!
//! In every case the bare state without the row releases both cards, so the vesting term is the one
//! deciding. The fence-off twin (`palw_rcore_plus` unset) releases them in every case.
//!
//! **Both halves of every gate.** A held card's spend is `SpendsNonReleasableBond` at the block
//! path and at the mempool; a released card's spend that claims its whole outpoint is
//! `BondBurnNotPaid { owed: SLASHED, left: 0 }` at both — the mempool's burn half (ADR-0109 D3,
//! `palw_mempool_bond_gate`) was added for this suite's review, which found the mempool admitting a
//! signed, burn-evading spend of a released, slashed bond that every template then dropped.
//!
//! No block is mined: the states are planted on the tip the harness chain stands at, and each site
//! is asked at the case's DAA (the wallet's set through `palw_locked_bond_outpoints_v2_at`, the
//! mempool through `validate_mempool_transaction_at_daa_for_tests`), because `F + 3,000` is a
//! chain 23,000 blocks long. The spends are of the cards' real genesis collateral outpoints,
//! unsigned: every gate asked here stands before the script check, so the gate's own error — or the
//! next check's — says which way it went.
use super::t12_round_lane_e2e::{T12Chain, t12_genesis_chain, t12_with_harness_cards};
use crate::pipeline::virtual_processor::utxo_validation::BondSpendFilter;
use crate::processes::transaction_validator::tx_validation_in_utxo_context::TxValidationFlags;
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::config::params::palw_v2_bond_withdrawal_delay_at_v1;
use kaspa_consensus_core::config::{Config, ConfigBuilder};
use kaspa_consensus_core::errors::tx::TxRuleError;
use kaspa_consensus_core::palw_economic_safety_v1::PalwLicenceDoorTagV1;
use kaspa_consensus_core::palw_panel_var_v1::palw_panel_liability_expiry_v1;
use kaspa_consensus_core::palw_state_v2::{
    PalwBondKeyV2, PalwBondStatusV2, PalwChainStateV2, PalwPayoutV2, PalwStateCarriageV2, PalwStateParamsV2,
    palw_bond_collateral_is_locked_v6,
};
use kaspa_consensus_core::palw_vesting_v1::{PalwVestingRowV1, palw_bond_is_payee_of_unmatured_row_v1, palw_chain_vesting_halted_v1};
use kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE;
use kaspa_consensus_core::tx::{MutableTransaction, Transaction, TransactionInput, TransactionOutput, UtxoEntry};
use kaspa_hashes::Hash64;

/// The row's `Final` DAA: far enough out that the cards' withdrawal delay (12,900 on testnet-12,
/// the DA lattice included) ran out long before `F + 2,999`, so only the row can hold them.
const F: u64 = 20_000;
/// The anchors the chain has settled in every planted state.
const SETTLED: u64 = 100;
/// What each retired card lost: a released bond's spend must leave this much unclaimed.
const SLASHED: u64 = 1_000;
/// The row's payees: its producer and a credited seat. Both retired and slashed, so the row is the
/// only hold on either — the payee index's producer entry and its seat entry are asked alike.
const PAYEE: usize = 3;
const SEAT: usize = 4;
const PAYEES: [usize; 2] = [PAYEE, SEAT];

struct Gate {
    chain: T12Chain,
    sink: BlockHash,
    tip: PalwChainStateV2,
    /// Each payee's collateral as the genesis premine holds it, by card.
    collateral: [(usize, UtxoEntry); 2],
}

/// One of the ADR's cases: the DAA it is asked at, the anchor ring, whether the row's second clock
/// has its licences, whether the row has latched, and what B-3 must answer.
struct Case {
    name: &'static str,
    now: u64,
    ring: Vec<u64>,
    licences_met: bool,
    latched: bool,
    halted: bool,
    held: bool,
}

async fn gate(armed: bool) -> Gate {
    kaspa_core::log::try_init_logger("warn");
    let (config, bundle, premine, floats) = t12_with_harness_cards();
    let (config, bundle) = if armed {
        (config, bundle)
    } else {
        // The fence-off twin: R-core+ unset, the bundle's mirrors re-synced (the fold and v6 read
        // `PalwStateParamsV2::rcore_plus_active_at`), everything else testnet-12.
        let mut params = config.params.clone();
        params.palw_rcore_plus = None;
        params.palw_rcore_conservative_classes = &[];
        params.sync_palw_rcore_plus();
        let config: Config = ConfigBuilder::new(params).skip_proof_of_work().build();
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(twin) = &config.params.palw_consensus_mode else {
            unreachable!("testnet-12 is ConsensusV2")
        };
        let twin = twin.clone();
        assert!(!twin.state.rcore_plus_active_at(F), "the twin's mirror is off");
        (config, twin)
    };
    let mut chain = t12_genesis_chain(&config, &bundle, &premine, &floats);
    chain.heartbeat(config.params.target_time_per_block(), Vec::new()).await;
    let (sink, tip) = chain.tip_state();
    let collateral = PAYEES.map(|card| {
        let outpoint = chain.bonds[card].0;
        let entry = premine.iter().find(|(o, _)| *o == outpoint).map(|(_, e)| e.clone());
        (card, entry.unwrap_or_else(|| panic!("card {card}'s collateral is a genesis UTXO")))
    });
    Gate { chain, sink, tip, collateral }
}

impl Gate {
    fn sp(&self) -> &PalwStateParamsV2 {
        &self.chain.bundle.state
    }

    fn window_court(&self) -> u64 {
        self.sp().window_court()
    }

    fn expiry(&self) -> u64 {
        palw_panel_liability_expiry_v1(F, self.window_court())
    }

    fn bond(&self, card: usize) -> PalwBondKeyV2 {
        self.chain.bonds[card]
    }

    /// The raw second-clock depth the processor passes at `daa` (`palw_settled_anchor_depth_at`).
    fn raw_depth(&self, daa: u64) -> Option<u64> {
        let p = &self.chain.config.params;
        if p.palw_audit_2026_09_23.is_some_and(|f| f.is_active(daa)) { p.palw_settled_anchor_depth } else { None }
    }

    fn depth(&self) -> u64 {
        self.chain.config.params.palw_settled_anchor_depth.expect("testnet-12 runs the second clock")
    }

    /// The row paying card 3 (producer) and card 4 (seat), `Final` at `F`.
    fn row(&self, case: &Case) -> PalwVestingRowV1 {
        let payload = |bond: &PalwBondKeyV2| self.tip.bond(bond).expect("a genesis card").payout_payload;
        let settled_at_final = if case.licences_met { SETTLED - self.depth() } else { SETTLED - self.depth() + 1 };
        let (producer, seat) = (self.bond(PAYEE), self.bond(SEAT));
        PalwVestingRowV1 {
            claim_id: Hash64::from_u64_word(0xB3_0023),
            producer_bond: producer,
            class_id: self.chain.bundle.base_class_id,
            execution_root: Hash64::from_u64_word(0xB3_00E7),
            artifact_root: Hash64::default(),
            job_identity: Hash64::default(),
            free_prompt: false,
            trace_root: Hash64::default(),
            segment_count: 0,
            licence_door: PalwLicenceDoorTagV1::Quorum,
            basis_k: 3,
            escrowed_reward: 1_000_000,
            buyback_bound: 0,
            producer: PalwPayoutV2 { payload: payload(&producer), amount: 800_000 },
            seats: vec![(seat, PalwPayoutV2 { payload: payload(&seat), amount: 150_000 })],
            reserve: 50_000,
            final_daa: F,
            expiry_daa: self.expiry(),
            settled_at_final,
            matured_at: case.latched.then_some(self.expiry()),
        }
    }

    /// The tip with cards 3 and 4 retired and slashed, the case's anchor ring and settled count,
    /// and — `with_row` — the row (its counters' `created` beside it, V-3).
    fn state(&self, case: &Case, with_row: bool) -> PalwChainStateV2 {
        let mut carriage = PalwStateCarriageV2::from_state(&self.tip);
        carriage.settled_attempt_finals = SETTLED;
        carriage.recent_anchor_daas = case.ring.clone();
        for card in PAYEES {
            let bond = carriage.bonds.get_mut(&self.bond(card)).unwrap_or_else(|| panic!("card {card} is registered"));
            bond.status = PalwBondStatusV2::Retiring { since_daa: 0, settled_at_since: 0 };
            bond.slashed = SLASHED;
        }
        if with_row {
            let row = self.row(case);
            carriage.vesting_counters.created += row.total_sompi_u128();
            carriage.vesting.insert(row.claim_id, row);
        }
        carriage.into_state(self.sp(), None).expect("the planted state is consistent")
    }

    /// `palw_bond_collateral_is_locked_v6` for `card`, with exactly the arguments the processor's
    /// `palw_v2_bond_is_locked` passes at `now`.
    fn v6(&self, state: &PalwChainStateV2, now: u64, card: usize) -> bool {
        let p = &self.chain.config.params;
        let delay = palw_v2_bond_withdrawal_delay_at_v1(&self.chain.bundle, p.palw_da_court, now);
        assert!(delay < F, "the withdrawal delay ({delay}) ran out before the row's Final: only the row can hold card {card}");
        let duty_gate = p.palw_audit_2026_09_23.is_some_and(|f| f.is_active(now));
        let key = self.bond(card);
        let bond = state.bond(&key).unwrap_or_else(|| panic!("card {card}"));
        palw_bond_collateral_is_locked_v6(state, self.sp(), &key, bond, now, delay, self.raw_depth(now), duty_gate)
    }

    /// A spend of `card`'s collateral that claims the whole outpoint (so a released bond's burn
    /// obligation is unpaid) — unsigned: every gate asked here stands before the script check.
    fn spend(&self, card: usize) -> Transaction {
        let (_, entry) = self.collateral.iter().find(|(c, _)| *c == card).expect("a payee card");
        Transaction::new(
            crate::constants::TX_VERSION,
            vec![TransactionInput::new(self.bond(card).0, vec![], 0, 1)],
            vec![TransactionOutput::new(entry.amount, entry.script_public_key.clone())],
            0,
            SUBNETWORK_ID_NATIVE,
            0,
            Vec::new(),
        )
    }

    fn cases(&self) -> Vec<Case> {
        let (e, w) = (self.expiry(), self.window_court());
        assert_eq!(e, F + 3_000, "testnet-12's conviction window: the row's DAA clock runs out at F + 3,000");
        let recent = |now: u64| vec![now - 10];
        let case =
            |name, now, ring, licences_met, latched, halted, held| Case { name, now, ring, licences_met, latched, halted, held };
        vec![
            case("F + 2,999", e - 1, recent(e - 1), true, false, false, true),
            // No anchor for `2 × window_court` before `E`: the halt drops the second clock, but the
            // DAA clock has not run — V-4(a) holds on it alone.
            case("a licence halt before E", e - 1, Vec::new(), false, false, true, true),
            case("F + 3,000, the licences in", e, recent(e), true, false, false, false),
            case("F + 3,000, one licence short", e, recent(e), false, false, false, true),
            case("F + 3,001, one licence short", e + 1, recent(e + 1), false, false, false, true),
            // No anchor for `2 × window_court` past `E`: the escape drops the second clock (V-4(b)
            // halts the latch; B-3 does not hold on it).
            case("a licence halt past E", e + 2, Vec::new(), false, false, true, false),
            // Latched at `E` and still in the table, waiting for V-7's budget.
            case("a carried row", e + 3, recent(e + 3), false, true, false, false),
            case("one DAA inside the per-obligation bound", e + 2 * w - 1, recent(e + 2 * w - 1), false, false, false, true),
            case("the per-obligation bound", e + 2 * w, recent(e + 2 * w), false, false, false, false),
        ]
    }

    /// Every site, asked about `state` at `now`, answers `want` for both payees.
    fn every_site_answers(&self, name: &str, state: &PalwChainStateV2, now: u64, want: bool) {
        let vp = self.chain.vp();

        // The block path's set, and the burn obligations beside it: the same predicate, negated.
        let locked = vp.palw_v2_locked_bond_outpoints(state, now);
        let burns = vp.palw_v2_bond_burn_obligations(state, now);
        for card in PAYEES {
            let outpoint = self.bond(card).0;
            assert_eq!(locked.contains(&outpoint), want, "{name}: card {card} in the block path's locked set");
            assert_eq!(
                burns.get(&outpoint).copied(),
                (!want).then_some(SLASHED),
                "{name}: card {card} — a released bond owes what it lost, a held one nothing yet"
            );
        }

        // The block path's per-transaction check, handed those two sets as the chain walk hands them.
        {
            let stores = vp.virtual_stores.read();
            let filter = BondSpendFilter::palw_only_for_tests(now, &locked, &burns);
            for card in PAYEES {
                let spend = self.spend(card);
                let spent =
                    vp.validate_transaction_in_utxo_context(&spend, &stores.utxo_set, now, TxValidationFlags::Full, Some(filter));
                self.gate_answered(name, "the block path", card, spent.map(|_| ()), want);
            }
        }

        // The wallet's set and the mempool read the tip: plant the state there. The mempool's cache
        // is keyed by (tip block, DAA), which two states planted at one block would alias — a
        // production tip block has one state — so it is emptied first.
        vp.palw_state_v2_store.write().set_tip_for_tests(self.sink, state).expect("the case becomes the tip");
        *vp.palw_mempool_locked_cache.lock() = None;
        let mut sorted: Vec<_> = locked.iter().copied().collect();
        sorted.sort_by(|a, b| (a.transaction_id, a.index).cmp(&(b.transaction_id, b.index)));
        assert_eq!(vp.palw_locked_bond_outpoints_v2_at(now), sorted, "{name}: the wallet's set is the block path's");
        for card in PAYEES {
            let mut tx = MutableTransaction::from_tx(self.spend(card));
            let admitted = vp.validate_mempool_transaction_at_daa_for_tests(&mut tx, now);
            self.gate_answered(name, "the mempool", card, admitted, want);
        }
    }

    /// One gate's answer to `card`'s whole-outpoint spend: the lock when held, the unpaid burn when
    /// released — both halves at both the block path and the mempool, so neither half can go quiet.
    fn gate_answered(&self, name: &str, site: &str, card: usize, answer: Result<(), TxRuleError>, want: bool) {
        match (answer, want) {
            (Err(TxRuleError::SpendsNonReleasableBond(outpoint)), true) => assert_eq!(outpoint, self.bond(card).0),
            (Err(TxRuleError::BondBurnNotPaid { owed, left }), false) => {
                assert_eq!((owed, left), (SLASHED, 0), "{name}: {site} charges card {card} what it lost")
            }
            (other, true) => panic!("{name}: {site} must refuse held card {card}'s collateral as locked, got {other:?}"),
            (other, false) => panic!("{name}: {site} must refuse released card {card}'s burn-evading spend, got {other:?}"),
        }
    }
}

/// **T23 (processor half) / T05 (bond half): the locked set, the burn obligations, the block path's
/// check, the mempool and the wallet's set all answer `palw_bond_collateral_is_locked_v6`** for the
/// row's producer and its seat, at `F + 2,999`, `F + 3,000` (with and without the second clock's
/// licences), `F + 3,001`, a licence halt on either side of `E`, a carried row, and both sides of
/// the per-obligation bound — and it is the vesting term that decides (the bare state releases both
/// cards in every case).
#[tokio::test]
async fn p2_t23_every_utxo_site_holds_a_vesting_payee_exactly_while_v4a_does() {
    let g = gate(true).await;
    assert!(g.sp().rcore_plus_active_at(F), "testnet-12 arms R-core+ from genesis");
    for case in g.cases() {
        let state = g.state(&case, true);
        let raw = g.raw_depth(case.now);
        assert_eq!(raw, Some(g.depth()), "{}: the processor passes the raw depth (I-8)", case.name);
        assert_eq!(palw_chain_vesting_halted_v1(&state, raw, case.now, g.window_court()), case.halted, "{}: the halt", case.name);
        let bare = g.state(&case, false);
        for card in PAYEES {
            assert_eq!(
                palw_bond_is_payee_of_unmatured_row_v1(&state, g.sp(), &g.bond(card), case.now, raw),
                case.held,
                "{}: B-3's vesting term for card {card}",
                case.name
            );
            assert_eq!(g.v6(&state, case.now, card), case.held, "{}: v6 for card {card}", case.name);
            assert!(
                !g.v6(&bare, case.now, card),
                "{}: without the row nothing holds card {card} — the vesting term decides",
                case.name
            );
        }
        g.every_site_answers(case.name, &state, case.now, case.held);
        g.every_site_answers(&format!("{} (no row)", case.name), &bare, case.now, false);
    }
    // The wallet's RPC answer is `_at` the node's own virtual DAA.
    let vp = g.chain.vp();
    let lkg = vp.lkg_virtual_state.load().daa_score;
    assert_eq!(vp.palw_locked_bond_outpoints_v2_impl(), vp.palw_locked_bond_outpoints_v2_at(lkg));
}

/// **The fence-off twin: below `palw_rcore_plus` B-3 has no vesting term** — the same planted
/// states release both cards at every site (v6 is v5 there), although the pure payee question still
/// says the row is unmatured: nothing below the fence reads it.
#[tokio::test]
async fn p2_t23_fence_off_twin_no_row_holds_a_bond_below_rcore_plus() {
    let g = gate(false).await;
    for case in g.cases() {
        let state = g.state(&case, true);
        for card in PAYEES {
            assert_eq!(
                palw_bond_is_payee_of_unmatured_row_v1(&state, g.sp(), &g.bond(card), case.now, g.raw_depth(case.now)),
                case.held,
                "{}: the pure question is fence-blind (card {card})",
                case.name
            );
            assert!(!g.v6(&state, case.now, card), "{}: below the fence v6 is v5 and releases card {card}", case.name);
        }
        g.every_site_answers(case.name, &state, case.now, false);
    }
}
