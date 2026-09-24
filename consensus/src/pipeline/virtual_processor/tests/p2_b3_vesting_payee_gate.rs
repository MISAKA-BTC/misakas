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
//! * the mempool's bond gate (`palw_mempool_locked_bonds`, memoised per tip and DAA);
//! * the wallet's set, `palw_locked_bond_outpoints_v2_impl` (the RPC's `locked_bond_outpoints`,
//!   which is how the hold reaches `misaka wallet` with no wallet change, F7).
//!
//! **The scenario.** Card 3 has retired long enough ago that its withdrawal delay and the
//! retirement's own second clock have both run, and it lost `SLASHED` sompi — so the ONLY thing that
//! can still hold its collateral is a vesting row naming it (as producer; card 4 sits on the row
//! too, but card 4 is Active and locked regardless). The row is `Final` at `F`, its DAA clock
//! runs out at `E = F + window_court` (3,000 on testnet-12), and its second clock needs
//! `palw_settled_anchor_depth` (30) anchors after `F`. The six cases are the ADR's: `F + 2,999`
//! (held), `F + 3,000` with the licences in (released), `F + 3,000` short of them (held by the
//! second clock), the same during a licence halt (released: B-3 does not hold on the halt), a
//! carried row (latched, not moved: released), and the per-obligation bound `E + 2 × window_court`
//! (released). In every case the bare state without the row releases the bond, so the vesting term
//! is the one deciding. The fence-off twin (`palw_rcore_plus` unset) releases it in all six.
//!
//! No block is mined: the states are planted on the tip the harness chain stands at, and each site
//! is asked at the case's DAA (the wallet's set through `palw_locked_bond_outpoints_v2_at`, the
//! mempool through `validate_mempool_transaction_at_daa_for_tests`), because `F + 3,000` is a
//! chain 23,000 blocks long. The spend is of card 3's real genesis collateral outpoint, unsigned:
//! every gate asked here stands before the script check, so the gate's own error — or the next
//! check's — says which way it went.
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

/// The row's `Final` DAA: far enough out that card 3's withdrawal delay (12,900 on testnet-12, the
/// DA lattice included) ran out long before `F + 2,999`, so only the row can hold it.
const F: u64 = 20_000;
/// The anchors the chain has settled in every planted state.
const SETTLED: u64 = 100;
/// What card 3 lost: a released bond's spend must leave this much unclaimed.
const SLASHED: u64 = 1_000;
/// The payee (producer) and a co-payee seat.
const PAYEE: usize = 3;
const SEAT: usize = 4;

struct Gate {
    chain: T12Chain,
    sink: BlockHash,
    tip: PalwChainStateV2,
    /// Card 3's collateral as the genesis premine holds it.
    collateral: UtxoEntry,
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
    let payee = chain.bonds[PAYEE];
    let collateral =
        premine.iter().find(|(o, _)| *o == payee.0).map(|(_, e)| e.clone()).expect("card 3's collateral is a genesis UTXO");
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

    fn payee(&self) -> PalwBondKeyV2 {
        self.chain.bonds[PAYEE]
    }

    fn seat(&self) -> PalwBondKeyV2 {
        self.chain.bonds[SEAT]
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
        PalwVestingRowV1 {
            claim_id: Hash64::from_u64_word(0xB3_0023),
            producer_bond: self.payee(),
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
            producer: PalwPayoutV2 { payload: payload(&self.payee()), amount: 800_000 },
            seats: vec![(self.seat(), PalwPayoutV2 { payload: payload(&self.seat()), amount: 150_000 })],
            reserve: 50_000,
            final_daa: F,
            expiry_daa: self.expiry(),
            settled_at_final,
            matured_at: case.latched.then_some(self.expiry()),
        }
    }

    /// The tip with card 3 retired and slashed, the case's anchor ring and settled count, and —
    /// `with_row` — the row (its counters' `created` beside it, V-3).
    fn state(&self, case: &Case, with_row: bool) -> PalwChainStateV2 {
        let mut carriage = PalwStateCarriageV2::from_state(&self.tip);
        carriage.settled_attempt_finals = SETTLED;
        carriage.recent_anchor_daas = case.ring.clone();
        let bond = carriage.bonds.get_mut(&self.payee()).expect("card 3 is registered");
        bond.status = PalwBondStatusV2::Retiring { since_daa: 0, settled_at_since: 0 };
        bond.slashed = SLASHED;
        if with_row {
            let row = self.row(case);
            carriage.vesting_counters.created += row.total_sompi_u128();
            carriage.vesting.insert(row.claim_id, row);
        }
        carriage.into_state(self.sp(), None).expect("the planted state is consistent")
    }

    /// `palw_bond_collateral_is_locked_v6` for card 3, with exactly the arguments the processor's
    /// `palw_v2_bond_is_locked` passes at `now`.
    fn v6(&self, state: &PalwChainStateV2, now: u64) -> bool {
        let p = &self.chain.config.params;
        let delay = palw_v2_bond_withdrawal_delay_at_v1(&self.chain.bundle, p.palw_da_court, now);
        assert!(delay < F, "the withdrawal delay ({delay}) ran out before the row's Final: only the row can hold card 3");
        let duty_gate = p.palw_audit_2026_09_23.is_some_and(|f| f.is_active(now));
        let bond = state.bond(&self.payee()).expect("card 3");
        palw_bond_collateral_is_locked_v6(state, self.sp(), &self.payee(), bond, now, delay, self.raw_depth(now), duty_gate)
    }

    /// A spend of card 3's collateral that claims the whole outpoint (so a released bond's burn
    /// obligation is unpaid) — unsigned: every gate asked here stands before the script check.
    fn spend(&self) -> Transaction {
        Transaction::new(
            crate::constants::TX_VERSION,
            vec![TransactionInput::new(self.payee().0, vec![], 0, 1)],
            vec![TransactionOutput::new(self.collateral.amount, self.collateral.script_public_key.clone())],
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
        vec![
            Case { name: "F + 2,999", now: e - 1, ring: recent(e - 1), licences_met: true, latched: false, halted: false, held: true },
            Case {
                name: "F + 3,000, the licences in",
                now: e,
                ring: recent(e),
                licences_met: true,
                latched: false,
                halted: false,
                held: false,
            },
            Case {
                name: "F + 3,000, one licence short",
                now: e + 1,
                ring: recent(e + 1),
                licences_met: false,
                latched: false,
                halted: false,
                held: true,
            },
            // No anchor for `2 × window_court`: the escape drops the second clock (V-4(b) halts the
            // latch; B-3 does not hold on it).
            Case {
                name: "a licence halt",
                now: e + 2,
                ring: Vec::new(),
                licences_met: false,
                latched: false,
                halted: true,
                held: false,
            },
            // Latched at `E` and still in the table, waiting for V-7's budget.
            Case {
                name: "a carried row",
                now: e + 3,
                ring: recent(e + 3),
                licences_met: false,
                latched: true,
                halted: false,
                held: false,
            },
            Case {
                name: "the per-obligation bound",
                now: e + 2 * w,
                ring: recent(e + 2 * w),
                licences_met: false,
                latched: false,
                halted: false,
                held: false,
            },
        ]
    }

    /// Every site, asked about `state` at `now`, answers `want`.
    fn every_site_answers(&self, name: &str, state: &PalwChainStateV2, now: u64, want: bool) {
        let vp = self.chain.vp();
        let payee = self.payee();

        // The block path's set, and the burn obligations beside it: the same predicate, negated.
        let locked = vp.palw_v2_locked_bond_outpoints(state, now);
        assert_eq!(locked.contains(&payee.0), want, "{name}: the block path's locked set");
        assert!(locked.contains(&self.seat().0), "{name}: an Active seat is locked whatever its rows");
        let burns = vp.palw_v2_bond_burn_obligations(state, now);
        assert_eq!(
            burns.get(&payee.0).copied(),
            (!want).then_some(SLASHED),
            "{name}: a released bond owes what it lost, a held one nothing yet"
        );

        // The block path's per-transaction check, handed those two sets as the chain walk hands them.
        {
            let stores = vp.virtual_stores.read();
            let filter = BondSpendFilter::palw_only_for_tests(now, &locked, &burns);
            match vp.validate_transaction_in_utxo_context(&self.spend(), &stores.utxo_set, now, TxValidationFlags::Full, Some(filter))
            {
                Err(TxRuleError::SpendsNonReleasableBond(outpoint)) => {
                    assert!(want, "{name}: the block path refused a released bond's spend");
                    assert_eq!(outpoint, payee.0);
                }
                Err(TxRuleError::BondBurnNotPaid { owed, left }) => {
                    assert!(!want, "{name}: the block path let a held bond's spend reach the burn check");
                    assert_eq!((owed, left), (SLASHED, 0));
                }
                other => panic!("{name}: the block path answers neither the lock nor the burn: {:?}", other.map(|_| ())),
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
        let mut tx = MutableTransaction::from_tx(self.spend());
        match vp.validate_mempool_transaction_at_daa_for_tests(&mut tx, now) {
            Err(TxRuleError::SpendsNonReleasableBond(outpoint)) => {
                assert!(want, "{name}: the mempool refused a released bond's spend");
                assert_eq!(outpoint, payee.0);
            }
            other => assert!(!want, "{name}: the mempool must refuse a held payee's collateral, got {other:?}"),
        }
    }
}

/// **T23 (processor half) / T05 (bond half): the locked set, the burn obligations, the block path's
/// check, the mempool and the wallet's set all answer `palw_bond_collateral_is_locked_v6`** at
/// `F + 2,999`, `F + 3,000` (with and without the second clock's licences), during a licence halt,
/// for a carried row and at the per-obligation bound — and it is the vesting term that decides
/// (the bare state releases the bond in every case).
#[tokio::test]
async fn p2_t23_every_utxo_site_holds_a_vesting_payee_exactly_while_v4a_does() {
    let g = gate(true).await;
    assert!(g.sp().rcore_plus_active_at(F), "testnet-12 arms R-core+ from genesis");
    for case in g.cases() {
        let state = g.state(&case, true);
        let raw = g.raw_depth(case.now);
        assert_eq!(raw, Some(g.depth()), "{}: the processor passes the raw depth (I-8)", case.name);
        assert_eq!(palw_chain_vesting_halted_v1(&state, raw, case.now, g.window_court()), case.halted, "{}: the halt", case.name);
        assert_eq!(
            palw_bond_is_payee_of_unmatured_row_v1(&state, g.sp(), &g.payee(), case.now, raw),
            case.held,
            "{}: B-3's vesting term",
            case.name
        );
        assert_eq!(g.v6(&state, case.now), case.held, "{}: v6", case.name);
        let bare = g.state(&case, false);
        assert!(!g.v6(&bare, case.now), "{}: without the row nothing holds card 3 — the vesting term decides", case.name);
        g.every_site_answers(case.name, &state, case.now, case.held);
        g.every_site_answers(&format!("{} (no row)", case.name), &bare, case.now, false);
    }
    // The wallet's RPC answer is `_at` the node's own virtual DAA.
    let vp = g.chain.vp();
    let lkg = vp.lkg_virtual_state.load().daa_score;
    assert_eq!(vp.palw_locked_bond_outpoints_v2_impl(), vp.palw_locked_bond_outpoints_v2_at(lkg));
}

/// **The fence-off twin: below `palw_rcore_plus` B-3 has no vesting term** — the same six planted
/// states release card 3 at every site (v6 is v5 there), although the pure payee question still
/// says the row is unmatured: nothing below the fence reads it.
#[tokio::test]
async fn p2_t23_fence_off_twin_no_row_holds_a_bond_below_rcore_plus() {
    let g = gate(false).await;
    for case in g.cases() {
        let state = g.state(&case, true);
        assert_eq!(
            palw_bond_is_payee_of_unmatured_row_v1(&state, g.sp(), &g.payee(), case.now, g.raw_depth(case.now)),
            case.held,
            "{}: the pure question is fence-blind",
            case.name
        );
        assert!(!g.v6(&state, case.now), "{}: below the fence v6 is v5 and releases card 3", case.name);
        g.every_site_answers(case.name, &state, case.now, false);
    }
}
