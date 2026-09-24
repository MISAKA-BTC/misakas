//! **Below `palw_audit_2026_09_23` the panel room folds byte for byte as the parent did** — the
//! dormant-parity pin of 2026-09-24 audit #4 review (all items: every change is past the fence).
//!
//! One deterministic chain folded through the REAL fold (`apply_palw_transition_v7`) with the
//! fences testnet-11 resolves at each DAA (registry + ADR-0137 work target from 6,001; the
//! 2026-09-23 audit fence never), crossing the registry's grace and many span boundaries, with two
//! model classes admitting, attempts filling the panel budget, claims bound and LICENSED (the
//! state the rate rule treats differently), and readiness refreshed through the carriage.
//!
//! Every block dumps: the fold's verdict, the state root, every lifecycle row, op 186
//! (`palw_model_registry_read_v2`, with t11's arguments) and the class gate
//! (`palw_class_admits_claim_v1`) for each class. The review compiled the same file against
//! 4064364e (the parent of the rate rule) and e93be0f2 and found the 1,953-line, 686,879-byte
//! dumps byte-identical (`DefaultHasher` digest `c9c84def84e94960`); f8c91f19 dumps the same bytes.
//! The dump's BLAKE2b-256 is pinned below, so any change to what testnet-11 folds, reads or
//! refuses here turns this red. A change to a dumped type's `Debug` form does too: re-derive the
//! pin from the parent build then, never from the build under test. `PARITY_DUMP=<path>` writes
//! the dump.
//!
//! Run: cargo test -p kaspa-consensus-core --test panel_room_t11_dormant_parity
#![allow(dead_code)]

#[path = "dos_l5_common.rs"]
mod common;
use common::*;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_model_registry_v1::{PalwModelLifecycleV1, PalwSeatReadinessRowV1, palw_model_registry_read_v2};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBlockWorkV3, PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwConsensusObjectV2,
    PalwStateCarriageV2, PalwStateDeltaV2, PalwStateParamsV2, PalwStateV2Error, PalwTransitionExtrasV1, apply_palw_transition_v7,
    palw_class_admits_claim_v1, palw_operator_id_v2,
};
use kaspa_consensus_core::palw_work_target_v1::PalwWorkTargetFoldV1;
use std::fmt::Write as _;

fn t11() -> Params {
    Params::from(NetworkId::with_suffix(NetworkType::Testnet, 11))
}

/// The model classes of `p`'s genesis, and a work for each: the node takes testnet-11's model works
/// from its canonical class table (processor-side, not reachable from core), so the test hands the
/// fold two synthetic works — window 3 for the first class, window 2 for the second, the private
/// t12's configuration — identical in both builds.
fn model_works(p: &Params) -> Vec<(Hash64, kaspa_consensus_core::palw_model_registry_v1::PalwModelWorkV1)> {
    use kaspa_consensus_core::palw_model_registry_v1::PalwModelWorkV1;
    let base = bundle(p).base_class_id;
    let models: Vec<Hash64> = genesis_classes(p).iter().map(|c| c.0).filter(|id| *id != base).collect();
    let w = |vccu: u128| PalwModelWorkV1 {
        verification_ccu: vccu,
        economic_ccu_per_claim: 1_600_000_000_000,
        ops_supported: true,
        ..Default::default()
    };
    vec![(models[0], w(2_000_000_000_000)), (models[1], w(1_000_000_000_000))]
}

fn reg(p: &Params, daa: u64) -> Option<kaspa_consensus_core::palw_model_registry_v1::PalwModelRegistryFoldV1> {
    registry_fold(p, daa).map(|mut f| {
        for (id, w) in model_works(p) {
            f.genesis_works.insert(id, w);
        }
        f
    })
}

/// `palw_transition_extras_for` as far as core can rebuild it: the shared fixture's extras plus the
/// three processor-only fields this rule reads (the work target's switch and fold, the payout).
fn x(p: &Params, sp: &PalwStateParamsV2, daa: u64) -> PalwTransitionExtrasV1 {
    let mut e = extras(p, daa);
    e.model_registry = reg(p, daa);
    e.round_lane = p.palw_execution_lane_at(daa).map(|lane| kaspa_consensus_core::palw_execution_lane_v1::PalwExecLaneFoldV1 {
        schedule_span_daa: lane.schedule_span_daa_at(daa),
        ..Default::default()
    });
    e.work_target_active = p.palw_work_target_at(daa);
    e.economic_payout = p.palw_economic_payout_at(daa).map(|pay| pay.fold_v1(0));
    e.work_target = work_fold(p, sp, daa);
    e
}

fn work_fold(p: &Params, sp: &PalwStateParamsV2, daa: u64) -> Option<PalwWorkTargetFoldV1> {
    let works = reg(p, daa.max(p.palw_model_registry.map(|f| f.daa_score()).unwrap_or(0)))?.genesis_works;
    Some(PalwWorkTargetFoldV1 {
        rate_sompi_per_giga: p
            .palw_economic_payout_at(daa)
            .map(|pay| pay.rate_sompi_per_giga)
            .unwrap_or(kaspa_consensus_core::palw_work_target_v1::PALW_WORK_TARGET_SHADOW_RATE_SOMPI_PER_GIGA_V1),
        block_bits: 0,
        max_factor: sp.class_daa_max_factor(),
        works,
    })
}

fn go(
    p: &Params,
    sp: &PalwStateParamsV2,
    parent: &PalwChainStateV2,
    c: &PalwBlockContextV2,
    objects: &[PalwConsensusObjectV2],
    work: PalwBlockWorkV3<'_>,
    key: Hash64,
) -> Result<(PalwChainStateV2, PalwStateDeltaV2, Vec<(Hash64, String)>), PalwStateV2Error> {
    let f = flags(p, c.daa_score);
    apply_palw_transition_v7(
        parent,
        sp,
        None,
        c,
        objects,
        work,
        &[],
        key,
        f.unavailable_abstains,
        f.capability_bound,
        f.uncertified_weightless,
        f.da_court,
        &x(p, sp, c.daa_score),
    )
}

const BONDS: u64 = 8;
const RICH: u64 = 1_000_000_000_000_000; // 10,000,000 MSK

fn dump_block(out: &mut String, p: &Params, sp: &PalwStateParamsV2, s: &PalwChainStateV2, daa: u64, classes: &[Hash64]) {
    writeln!(out, "  root {}", s.state_root()).unwrap();
    for (id, row) in s.model_lifecycles_iter() {
        writeln!(out, "  row {id} {row:?}").unwrap();
    }
    let fold = reg(p, daa);
    let reg_daa = p.palw_model_registry.map(|f| f.daa_score());
    let enforced = p.palw_work_target_at(daa) && fold.as_ref().is_some_and(|f| f.governs_at(daa));
    let work = work_fold(p, sp, daa);
    let read = palw_model_registry_read_v2(
        s,
        sp,
        daa,
        reg_daa,
        fold.as_ref(),
        work.as_ref(),
        p.palw_audit_2026_09_23_active_at(daa),
        enforced,
    );
    writeln!(
        out,
        "  op186 work_target {:?} enforced {} fp_counted {}",
        read.work_target, read.panel_room_enforced, read.free_prompts_counted
    )
    .unwrap();
    for c in &read.classes {
        writeln!(
            out,
            "  op186 class {} room {} inflight {} ready_now {} share {:?} reason {:?}",
            c.class_id, c.panel_room, c.inflight_now, c.ready_seats_now, c.share_permille, c.reason
        )
        .unwrap();
    }
    for id in classes {
        let verdict = palw_class_admits_claim_v1(s, sp, &x(p, sp, daa + 1), id, daa + 1);
        writeln!(out, "  gate {id} {:?}", verdict.map_err(|e| e.to_string())).unwrap();
    }
}

fn refresh_readiness(sp: &PalwStateParamsV2, s: &PalwChainStateV2, daa: u64, span: u64, models: &[Hash64]) -> PalwChainStateV2 {
    let mut c = PalwStateCarriageV2::from_state(s);
    for n in 1..=BONDS {
        for id in models {
            c.seat_readiness.insert(
                (bond_key(n), *id),
                PalwSeatReadinessRowV1 { proved_daa: daa, proved_span: daa / span.max(1), leaf_index: 0, proof_version: 2, chunks: 8 },
            );
        }
    }
    c.into_state(sp, None).expect("a refreshed readiness carriage is consistent")
}

/// Runs the scripted chain and returns its dump.
fn run(p: &Params, force_audit_off: bool) -> String {
    let b = bundle(p);
    let sp = b.state.clone();
    let seat_count = b.panel.seat_count() as usize;
    let classes: Vec<Hash64> = genesis_classes(p).iter().map(|c| c.0).collect();
    let models: Vec<Hash64> = classes.iter().copied().filter(|id| *id != b.base_class_id).collect();
    assert!(models.len() >= 2, "two model classes to interact: {models:?}");
    let reg_daa = p.palw_model_registry.map(|f| f.daa_score()).expect("the registry is scheduled");
    let mut out = String::new();
    writeln!(out, "net {:?} seat_count {seat_count} registry at {reg_daa} audit_off {force_audit_off}", p.net).unwrap();
    for n in [NetworkType::Mainnet, NetworkType::Devnet] {
        let q = Params::from(NetworkId::new(n));
        assert!(!q.palw_audit_2026_09_23_active_at(0) && !q.palw_audit_2026_09_23_active_at(u64::MAX), "{n:?} arms the audit fence");
    }
    assert!(!p.palw_audit_2026_09_23_active_at(u64::MAX), "t11 never arms the audit fence");
    let mut s = genesis_state(p);
    writeln!(out, "genesis root {}", s.state_root()).unwrap();
    let mut blue = 1u64;
    let start = reg_daa - 1;
    let objs: Vec<PalwConsensusObjectV2> = (1..=BONDS).map(|n| bond_obj(n, RICH)).collect();
    s = go(p, &sp, &s, &ctx(0x5000_0000, start, blue, 0), &objs, PalwBlockWorkV3::None, Hash64::default()).expect("bonds").0;
    let mut opened = false;
    let mut seed = 0u64;
    let mut accepted = 0u32;
    let mut refused: std::collections::BTreeMap<String, u32> = Default::default();
    let mut licensed = 0u32;
    let mut held_seen = 0u32;
    let end = reg_daa + 300;
    let mut daa = start + 1;
    while daa <= end {
        blue += 1;
        let span = reg(p, daa).map(|f| f.span_daa).unwrap_or(5);
        if s.model_lifecycles_iter().count() > 0 {
            if !opened {
                opened = true;
                // The first rows: one model class admitting in full, the other where the fold opened it.
                let mut c = PalwStateCarriageV2::from_state(&s);
                c.model_lifecycles.get_mut(&models[0]).expect("row").state = PalwModelLifecycleV1::Active;
                s = c.into_state(&sp, None).expect("consistent");
                writeln!(out, "rows opened by daa {daa}").unwrap();
            }
            // The second (shorter-window) class's seats prove possession only late in the chain, so it
            // enters admission while the first class already has claims in flight — the private
            // t12's sequence.
            let proving: Vec<Hash64> = if daa >= reg_daa + 220 { models.clone() } else { vec![models[0]] };
            s = refresh_readiness(&sp, &s, daa - 1, span, &proving);
        }
        // Bind, then license, the oldest provisional/bound model claim every 7th block.
        let mut objects: Vec<PalwConsensusObjectV2> = Vec::new();
        if daa % 7 == 0 {
            let bound = s
                .claims_iter()
                .filter(|(_, c)| models.contains(&c.class_id) && matches!(c.phase, PalwClaimPhaseV2::PanelBound { .. }))
                .map(|(id, c)| (*id, c.clone()))
                .next();
            if let Some((id, _)) = bound {
                let seats: Vec<PalwBondKeyV2> =
                    s.panel(&id).map(|panel| panel.seats.iter().map(|seat| seat.bond).collect()).unwrap_or_default();
                objects.push(PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts: valid_receipts(id, &seats) });
            } else if let Some((id, claim)) = s
                .claims_iter()
                .filter(|(_, c)| models.contains(&c.class_id) && matches!(c.phase, PalwClaimPhaseV2::Provisional))
                .map(|(id, c)| (*id, c.clone()))
                .next()
            {
                let seats: Vec<(PalwBondKeyV2, Hash64)> = (1..=BONDS)
                    .map(bond_key)
                    .filter(|k| *k != claim.bond)
                    .take(seat_count)
                    .map(|k| {
                        let n = (1..=BONDS).find(|n| bond_key(*n) == k).unwrap();
                        (k, palw_operator_id_v2(&operator_pubkey_of(n)))
                    })
                    .collect();
                objects.push(PalwConsensusObjectV2::PanelBound { claim: id, anchor: h(0x7DA0 + daa), seats: seats_of(&seats) });
            }
        }
        // The block's own attempt: model 0 twice, model 1 once, rotating producers.
        seed += 1;
        let class = if seed % 3 == 0 { models[1] } else { models[0] };
        let producer = 1 + (seed % BONDS);
        let pwu = s.class_target(&class).map(|_| 1_000u64).unwrap_or(1_000);
        let (env, key, _id) = junk_attempt(
            class,
            bond_key(producer),
            pubkey_of(producer),
            &operator_pubkey_of(producer),
            pwu,
            0x5EED_0000 + seed,
            0xBEEF_0000 + seed,
        );
        let c = ctx(0x5000_0000 + daa, daa, blue, T12_BLOCK_SUBSIDY_SOMPI);
        writeln!(out, "block daa {daa} class {class} producer {producer} objects {}", objects.len()).unwrap();
        let mut result = go(p, &sp, &s, &c, &objects, PalwBlockWorkV3::Attempt(&env), key);
        if let Err(e) = &result {
            let name = format!("{e:?}").split(['{', '(', ' ']).next().unwrap_or("").to_string();
            *refused.entry(name).or_default() += 1;
            writeln!(out, "  attempt refused: {e}").unwrap();
            result = go(p, &sp, &s, &c, &objects, PalwBlockWorkV3::None, Hash64::default());
            if let Err(e) = &result {
                writeln!(out, "  objects refused too: {e}").unwrap();
                result = go(p, &sp, &s, &c, &[], PalwBlockWorkV3::None, Hash64::default());
            }
        } else {
            accepted += 1;
        }
        let (next, _, skips) = result.expect("an empty block always folds");
        if !skips.is_empty() {
            writeln!(out, "  skips {skips:?}").unwrap();
        }
        s = next;
        licensed =
            licensed.max(s.claims_iter().filter(|(_, c)| matches!(c.phase, PalwClaimPhaseV2::ReceiptLicensed { .. })).count() as u32);
        held_seen += s.model_lifecycles_iter().filter(|(_, r)| r.state == PalwModelLifecycleV1::Held).count() as u32;
        dump_block(&mut out, p, &sp, &s, daa, &classes);
        daa += 2;
    }
    writeln!(out, "summary accepted {accepted} refused {refused:?} max_licensed_live {licensed} held_row_blocks {held_seen}").unwrap();
    println!("summary accepted {accepted} refused {refused:?} max_licensed_live {licensed} held_row_blocks {held_seen}");
    out
}

/// The dump the parent (4064364e) writes, as its BLAKE2b-256, its length and its line count.
///
/// **Re-pinned once for ADR-0152's v22 skeleton**, whose `PALW_STATE_V2_VERSION` 21 -> 22 and claim
/// record appends move every state root the dump prints (`genesis root …`, `  root …`) and nothing
/// else it prints: the length and the line count are the parent's to the byte, as they must be when
/// only fixed-width 128-hex roots changed. Derived from the build under test (the parent cannot print
/// a v22 root); the v21 digest was `50f81bb0…`.
///
/// **And once more for the v22 layout's `PalwExecFinalV1::accepted_blue_score`** (ADR-0152 v3.1, the
/// Phase 1–2 review's M-1): 0 on every Final below `palw_offence_attribution`, so the fold is
/// unchanged, but eight bytes more per rooted Final record. Measured: with the field
/// `#[borsh(skip)]` the dump is `8254eab6…` to the byte; serialized, exactly 11 `  root` lines
/// differ (the blocks holding a Final) and nothing else — length and line count unchanged.
const PARENT_DUMP_BLAKE2B_256: &str = "7bf3238f4394776c077b19cc4b555909f672060dbb4d177ac5660c441c50b0d2";
const PARENT_DUMP_BYTES: usize = 686_879;
const PARENT_DUMP_LINES: usize = 1_953;

fn digest(s: &str) -> String {
    blake2b_simd::Params::new().hash_length(32).hash(s.as_bytes()).to_hex().to_string()
}

#[test]
fn below_the_audit_fence_the_panel_room_folds_as_the_parent_did() {
    let p = t11();
    let dump = run(&p, false);
    println!("t11 dump: {} bytes, {} lines, BLAKE2b-256 {}", dump.len(), dump.lines().count(), digest(&dump));
    if let Ok(path) = std::env::var("PARITY_DUMP") {
        std::fs::write(&path, &dump).expect("write the dump");
        println!("written to {path}");
    }
    // The chain actually exercised the dormant room: attempts were admitted, some refused by the
    // room, claims licensed, and the lifecycle held a class (the dormant defect, verbatim).
    assert!(dump.contains("attempt refused"), "the budget filled at least once");
    assert_eq!((dump.len(), dump.lines().count()), (PARENT_DUMP_BYTES, PARENT_DUMP_LINES), "the parent's dump, line for line");
    assert_eq!(digest(&dump), PARENT_DUMP_BLAKE2B_256, "the parent's dump, byte for byte");
}
