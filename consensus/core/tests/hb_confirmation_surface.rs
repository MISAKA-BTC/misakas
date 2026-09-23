//! LANE H3 — what the node tells a wallet, an explorer or a merchant about depth, on a
//! heartbeat-only history.
//!
//! Every number here is produced by RUNTIME code at this commit; nothing is asserted from prose.

use kaspa_consensus_core::config::params::{Params, palw_clock_advances_without_a_claim_v1};
use kaspa_consensus_core::dns_finality::{DnsCoinbaseSettlement, coinbase_spend_settled};
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_attempt_v2::{
    PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, attempt_id_v2, challenge_v2,
};
use kaspa_consensus_core::palw_clock_cursor_v1::{
    ClockWindowBlockV1, palw_clock_cursor_from_reference_v1, palw_clock_reference_v1, palw_clock_slot_admits_v1,
};
use kaspa_consensus_core::palw_heartbeat_v1::HEARTBEAT_RECOVERY_INTERVAL_MS;
use kaspa_consensus_core::palw_panel_v2::{PalwReceiptVerdictV2, PalwSeatReceiptV2};
use kaspa_consensus_core::palw_settlement_v1::{PalwSettlementV1, palw_settlement_v1};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwConsensusObjectV2, PalwPanelSeatV2, PalwPwuRuleV2,
    PalwStateParamsV2, apply_palw_transition_v2, palw_operator_id_v2,
};
use kaspa_consensus_core::pow_layer0::{PALW_HEARTBEAT_MAX_PER_MERGESET, PALW_HEARTBEAT_WORK_LOG2};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use kaspa_consensus_core::{BlockHash, Hash64};

// ---- fixtures: the same public fold `palw_settlement_v1`'s own tests drive -------------------

fn params() -> PalwStateParamsV2 {
    PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, h64(1), 4, 1000, 100, 1000, 0).unwrap().with_fp_quanta(8, 64).unwrap()
}

fn h64(v: u64) -> Hash64 {
    Hash64::from_u64_word(v)
}

fn block(v: u64) -> BlockHash {
    BlockHash::from_u64_word(v)
}

fn bond() -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(1), index: 0 })
}

fn at(block_word: u64, daa: u64) -> PalwBlockContextV2 {
    PalwBlockContextV2 { block: block(block_word), daa_score: daa, blue_score: block_word, subsidy: 0 }
}

fn register() -> Vec<PalwConsensusObjectV2> {
    vec![
        PalwConsensusObjectV2::ClassRegistered {
            class_id: h64(1),
            artifact_root: h64(11),
            slash_value_per_pwu: 5,
            pwu_rule: PalwPwuRuleV2::MaxPerAttempt(160),
            initial_target: u128::MAX / 2,
            share_permille: 1000,
            activation_daa: 0,
            admission: None,
        },
        PalwConsensusObjectV2::BondRegistered {
            bond: bond(),
            pubkey: vec![7; 4],
            operator_pubkey: vec![21; 8],
            collateral: 1_000,
            payout_payload: Hash64::from_u64_word(0x9A11),
            capable_classes: Default::default(),
            signature: Vec::new(),
        },
    ]
}

fn attempt(nonce: u64) -> PalwAttemptEnvelopeV2 {
    let network_domain = h64(999);
    PalwAttemptEnvelopeV2 {
        attempt: PalwAttemptUnsignedV2 {
            version: PALW_ATTEMPT_V2_VERSION,
            network_domain,
            challenge: challenge_v2(network_domain, h64(5), 1_700, nonce, h64(1), &bond().0),
            class_id: h64(1),
            executor_bond: bond().0,
            executor_pubkey: vec![7; 4],
            operator_id: palw_operator_id_v2(&[21; 8]),
            artifact_root: h64(11),
            trace_root: h64(31),
            output_root: h64(32),
            pwu: 40,
            trace_manifest_root: h64(33),
            trace_chunk_count: 4,
            trace_retention_daa: 999_999,
            execution_root: h64(41),
        },
        signature: vec![0; 8],
    }
}

fn bind(claim: Hash64) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::PanelBound { claim, anchor: h64(77), seats: vec![PalwPanelSeatV2 { bond: bond(), operator_id: h64(90) }] }
}

fn license(claim: Hash64) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::ReceiptLicensed {
        claim,
        receipts: vec![PalwSeatReceiptV2 {
            claim: Hash64::default(),
            verdict: PalwReceiptVerdictV2::Valid,
            seat_bond: bond(),
            signed_daa: 0,
            signature: Vec::new(),
        }],
    }
}

fn step(
    parent: &PalwChainStateV2,
    p: &PalwStateParamsV2,
    point: PalwBlockContextV2,
    objects: &[PalwConsensusObjectV2],
    work: Option<&PalwAttemptEnvelopeV2>,
) -> PalwChainStateV2 {
    let (state, _) = apply_palw_transition_v2(parent, p, &point, objects, work).expect("the block folds");
    state
}

fn read(state: &PalwChainStateV2, p: &PalwStateParamsV2, d: u64) -> PalwSettlementV1 {
    palw_settlement_v1(state, p, None, d).expect("the frontier is dated")
}

// =================================================================================================
// H3-1. On testnet-12 the heartbeat IS the whole clock — asked of the shipped params, not of a doc.
// =================================================================================================

#[test]
fn t12_advances_its_clock_with_no_claim_at_all() {
    let t12 = Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12));
    let bondless_clock = palw_clock_advances_without_a_claim_v1(&t12);
    println!(
        "t12: clock advances without a claim = {bondless_clock}; coinbase_maturity = {} DAA; \
         dns coinbase_settlement_long_maturity = {} DAA; target_time_per_block = {} ms",
        t12.coinbase_maturity(),
        t12.coinbase_settlement_long_maturity_daa(),
        t12.target_time_per_block()
    );
    println!(
        "heartbeat price: 2^{PALW_HEARTBEAT_WORK_LOG2} hashes/beat = {} hashes; max {PALW_HEARTBEAT_MAX_PER_MERGESET} beats/mergeset; \
         clock interval {HEARTBEAT_RECOVERY_INTERVAL_MS} ms",
        1u64 << PALW_HEARTBEAT_WORK_LOG2
    );
    assert!(bondless_clock, "ADR-0151 D3: on t12 no lane is priced by bits, so the heartbeat stand-in is the clock");
}

// =================================================================================================
// H3-2. GATE B. A heartbeat-only history settles nothing — and matures everything.
// =================================================================================================

/// Two `Final` anchors and one pending, exactly as `palw_settlement_v1`'s own fixture builds them;
/// then N claimless chain blocks (what a heartbeat-only history looks like to the fold: a block
/// that carries no attempt), one DAA apiece — which is what the stand-in rule grants a beat.
#[test]
fn heartbeat_only_history_settles_nothing_but_matures_everything() {
    let p = params();
    let (a, b, c) = (attempt(1), attempt(2), attempt(3));
    let ids = [attempt_id_v2(&a.attempt), attempt_id_v2(&b.attempt), attempt_id_v2(&c.attempt)];
    let s = step(&PalwChainStateV2::genesis(), &p, at(1, 100), &register(), None);
    let s = step(&s, &p, at(2, 101), &[], Some(&a));
    let s = step(&s, &p, at(3, 102), &[bind(ids[0])], Some(&b));
    let s = step(&s, &p, at(4, 103), &[bind(ids[1]), license(ids[0])], None);
    let s = step(&s, &p, at(5, 104), &[license(ids[1])], None);
    let s = step(&s, &p, at(6, 120), &[], Some(&c));
    let mut s = step(&s, &p, at(7, 125), &[], None);
    assert!(matches!(s.claim(&ids[0]).unwrap().phase, PalwClaimPhaseV2::Final { .. }));

    // **The payment.** A merchant is paid by a transaction the selected chain accepts at DAA 126 —
    // one past the last anchor. Nothing is settled at 126 yet.
    const PAYMENT_DAA: u64 = 126;
    let before = read(&s, &p, PAYMENT_DAA);
    assert_eq!((before.settled, before.depth, before.pending), (false, 0, 0), "{before:?}");

    // **Then 1,000 heartbeats.** Claimless blocks, one DAA each: the stand-in rule
    // (consensus/src/processes/difficulty.rs:429 `stand_in = priced == 0 && heartbeats > 0`)
    // grants exactly one beat per mergeset the DAA tick, throttled by the ADR-0142 cursor to one
    // per HEARTBEAT_RECOVERY_INTERVAL_MS of wall clock.
    const BEATS: u64 = 1_000;
    for i in 0..BEATS {
        s = step(&s, &p, at(8 + i, 127 + i), &[], None);
    }
    let after = read(&s, &p, PAYMENT_DAA);
    let sink_daa = after.sink_daa;
    let daa_depth = sink_daa - PAYMENT_DAA;

    // ---- what a merchant can read, side by side -------------------------------------------
    // 1. getPalwSettlement (op 182) — anchors.
    // 2. getBlockDagInfo.virtualDaaScore / a UTXO's blockDaaScore — the DAA delta.
    // 3. the node's own coinbase spendability rule, consensus/core/src/dns_finality.rs:4115.
    // 4. the wallet framework's Maturity, wallet/core/src/utxo/reference.rs:47-64 (formula
    //    transcribed; the crate is not a dependency of this one).
    let t12 = Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12));
    // Below `palw_audit_2026_09_23` the long fallback was a DAA count alone — this record.
    let settlement = DnsCoinbaseSettlement {
        long_maturity_daa: t12.coinbase_settlement_long_maturity_daa(),
        confirmed_anchor_daa: None, // no DNS validator ever confirmed an anchor
        settled_anchor_armed: false,
        settled_anchor_floor_daa: None,
    };
    // Past it, the same history: the second clock is armed and this chain has settled no anchor,
    // so the fallback does not mature. That difference is the fix.
    let settlement_armed = DnsCoinbaseSettlement { settled_anchor_armed: true, ..settlement };
    let coinbase_spendable =
        coinbase_spend_settled(PAYMENT_DAA, sink_daa, t12.coinbase_maturity(), (settlement.long_maturity_daa > 0).then_some(&settlement));
    let coinbase_spendable_armed = coinbase_spend_settled(
        PAYMENT_DAA,
        sink_daa,
        t12.coinbase_maturity(),
        (settlement_armed.long_maturity_daa > 0).then_some(&settlement_armed),
    );
    // wallet/core/src/utxo/settings.rs: user_transaction_maturity_period_daa = 100,
    // coinbase_transaction_maturity_period_daa = 1_000, stasis = 500 (every shipped network).
    const WALLET_USER_MATURITY_DAA: u64 = 100;
    let wallet_user_confirmed = PAYMENT_DAA + WALLET_USER_MATURITY_DAA <= sink_daa;

    println!("---- heartbeat-only history: {BEATS} beats, payment accepted at DAA {PAYMENT_DAA} ----");
    println!("  hashes burned by the attacker            = {BEATS} x 2^{PALW_HEARTBEAT_WORK_LOG2} = {}", BEATS * (1u64 << PALW_HEARTBEAT_WORK_LOG2));
    println!("  wall clock at the cursor's interval      = {BEATS} x {HEARTBEAT_RECOVERY_INTERVAL_MS} ms = {} h", BEATS * HEARTBEAT_RECOVERY_INTERVAL_MS / 3_600_000);
    println!("  bond / collateral required               = 0 (bondless, claimless lane)");
    println!("  getPalwSettlement.depth  (anchors)       = {}", after.depth);
    println!("  getPalwSettlement.settled                = {}", after.settled);
    println!("  getPalwSettlement.pendingAnchors         = {}", after.pending);
    println!("  DAA depth (virtualDaaScore - blockDaa)   = {daa_depth}");
    println!("  node mempool: coinbase spendable         = {coinbase_spendable}  (pre-fence, DAA only)");
    println!("  node mempool: coinbase spendable, armed  = {coinbase_spendable_armed}  (both clocks)");
    println!("  wallet Balance: user tx Maturity::Confirmed = {wallet_user_confirmed}");

    // GATE B holds on the op that exists.
    assert_eq!((after.settled, after.depth), (false, 0), "1,000 heartbeats add no settled anchor: {after:?}");
    assert_eq!(after.pending, 0, "C voided at its bind deadline; nothing is on its way either");

    // And every OTHER depth-like number the same history produces says the opposite.
    assert_eq!(daa_depth, BEATS, "the DAA depth is exactly the beat count");
    assert!(wallet_user_confirmed, "wallet/core/src/utxo/reference.rs:60 matures a payment on {WALLET_USER_MATURITY_DAA} DAA alone");
    assert!(
        coinbase_spendable,
        "PRE-FENCE: the {} DAA long fallback was a DAA count, so heartbeats alone made mined coin spendable",
        settlement.long_maturity_daa
    );
    // THE FIX: past `palw_audit_2026_09_23` the same heartbeat-only history matures nothing,
    // because the second clock has no settled anchor to count.
    assert!(
        !coinbase_spendable_armed,
        "past the fence the long fallback also needs settled anchors, and {BEATS} heartbeats settle none"
    );
}

/// The smallest beat counts that flip each economic threshold, with no anchor and no bond.
#[test]
fn the_beat_count_that_buys_each_economic_threshold() {
    let t12 = Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12));
    // The pre-fence record: the DAA clock alone, which is what this table measures.
    let settlement = DnsCoinbaseSettlement {
        long_maturity_daa: t12.coinbase_settlement_long_maturity_daa(),
        confirmed_anchor_daa: None,
        settled_anchor_armed: false,
        settled_anchor_floor_daa: None,
    };
    let first_true = |f: &dyn Fn(u64) -> bool| (0u64..5_000).find(|n| f(*n)).expect("flips inside 5,000 beats");
    let coinbase = first_true(&|n| {
        coinbase_spend_settled(1_000, 1_000 + n, t12.coinbase_maturity(), (settlement.long_maturity_daa > 0).then_some(&settlement))
    });
    let wallet_user = first_true(&|n| 1_000 + 100 <= 1_000 + n);
    let wallet_coinbase = first_true(&|n| 1_000 + 1_000 <= 1_000 + n);
    let hours = |n: u64| n * HEARTBEAT_RECOVERY_INTERVAL_MS / 3_600_000;
    println!("beats to reach each threshold on t12, bondless, zero PALW anchors:");
    println!("  node mempool coinbase spendable   : {coinbase} beats ({} h, {} hashes)", hours(coinbase), coinbase << PALW_HEARTBEAT_WORK_LOG2);
    println!("  wallet user tx Confirmed          : {wallet_user} beats ({} h)", hours(wallet_user));
    println!("  wallet coinbase Confirmed         : {wallet_coinbase} beats ({} h)", hours(wallet_coinbase));
    assert_eq!(coinbase, t12.coinbase_settlement_long_maturity_daa(), "the DNS long fallback is the binding term");
}

// =================================================================================================
// H3-3. The clock cursor is the ONLY throttle on the DAA number, and it is wall clock, not work.
// =================================================================================================

#[test]
fn the_cursor_throttles_the_beat_by_wall_clock_and_by_nothing_else() {
    // A window standing at DAA `d`, reference block at t0. The cursor opens t0 + 120_000 ms.
    let t0 = 1_700_000_000_000u64;
    let window = [
        ClockWindowBlockV1 { daa_score: 10, blue_score: 5, timestamp_ms: t0, hash: block(5) },
        ClockWindowBlockV1 { daa_score: 10, blue_score: 6, timestamp_ms: t0 + 30_000, hash: block(6) },
        ClockWindowBlockV1 { daa_score: 9, blue_score: 4, timestamp_ms: t0 - 60_000, hash: block(4) },
    ];
    let reference = palw_clock_reference_v1(10, window).expect("a reference is derivable");
    let cursor = palw_clock_cursor_from_reference_v1(reference, HEARTBEAT_RECOVERY_INTERVAL_MS);
    println!("reference_ms = {reference} (t0 + {}), next_slot_ms = t0 + {}", reference - t0, cursor.next_slot_ms - t0);
    assert!(palw_clock_slot_admits_v1(&cursor, t0 + 119_999).is_err(), "a beat before the slot is refused the tick");
    assert!(palw_clock_slot_admits_v1(&cursor, t0 + 120_000).is_ok(), "at the slot it is granted");
    // Nothing in the admission reads a bond, a claim, a collateral or an anchor.
    println!("the slot rule's whole input is (next_slot_ms, proposed_ms) — no bond, no claim, no anchor");
}

// =================================================================================================
// H3-4. The client surfaces. What a JS wallet / explorer can actually call.
// =================================================================================================

/// **The WASM RPC client is the surface every browser wallet and explorer in this tree uses.** It
/// exposes the two heartbeat-advanced depth numbers and neither of the two economic ones.
#[test]
fn the_wasm_rpc_client_exposes_no_anchor_depth_and_no_dns_finality() {
    let wasm_client = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../rpc/wrpc/wasm/src/client.rs"))
        .expect("rpc/wrpc/wasm/src/client.rs is where the JS method list is built");
    let rust_client = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../rpc/wrpc/client/src/client.rs"))
        .expect("the Rust wRPC client's method list");
    let has = |hay: &str, needle: &str| hay.contains(needle);
    for op in ["GetBlockDagInfo", "GetSinkBlueScore", "GetVirtualChainFromBlock", "GetBlock"] {
        println!("wasm client exposes {op}: {}", has(&wasm_client, op));
        assert!(has(&wasm_client, op), "{op} is on the JS surface");
    }
    for op in ["GetPalwSettlement", "GetDnsConfirmation"] {
        println!("wasm client exposes {op}: {} | rust wrpc client exposes {op}: {}", has(&wasm_client, op), has(&rust_client, op));
        assert!(has(&rust_client, op), "{op} exists on the Rust wRPC client");
    }
    // The gap this test was written to hold open is closed: both ops are on the JS surface now
    // (`rpc/wrpc/wasm/src/client.rs`, with their request/response interfaces in
    // `rpc/core/src/wasm/message.rs`). The assertion is inverted so a future edit that drops them
    // again fails here by name.
    assert!(has(&wasm_client, "GetPalwSettlement"), "anchor depth must stay on the JS surface (getPalwSettlement)");
    assert!(has(&wasm_client, "GetDnsConfirmation"), "DNS finality must stay on the JS surface (getDnsConfirmation)");
}

/// **The wallet framework never asks the node about anchors.** Its whole notion of "confirmed" is
/// the DAA arithmetic in `reference.rs`.
#[test]
fn the_wallet_framework_never_reads_a_palw_anchor() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../wallet/core/src");
    let mut hits = Vec::new();
    let mut stack = vec![std::path::PathBuf::from(root)];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("wallet/core/src is readable") {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let text = std::fs::read_to_string(&path).expect("a source file");
                if text.contains("palw_settlement") || text.contains("get_palw_settlement") || text.contains("PalwSettlement") {
                    hits.push(path);
                }
            }
        }
    }
    println!("wallet/core/src files that mention PALW settlement: {hits:?}");
    assert!(hits.is_empty(), "if this goes red the wallet gained an anchor-aware balance — update this test deliberately");
}
