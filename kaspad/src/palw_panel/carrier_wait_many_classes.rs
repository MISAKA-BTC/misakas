//! **RFC-0018 (iv): the wait one Panel service's single in-flight carrier (`MAX_INFLIGHT_CARRIERS = 1`) imposes across many classes**
//! (2026-10-18). The tick's own scheduler (`PalwCarrierSlotsV1`, `TICK_ORDER`) is driven over a synthetic duty load: a seat sitting on
//! `K` classes answers, per class, one panel duty every `DUTY_EVERY` DAA (its receipt goes to the collector off chain); it is the
//! collector — one licence carrier — for one duty in `PANEL`, and carries a supplementary own receipt (ADR-0124 D2) for another one
//! in `PANEL`, both due inside testnet-12's `window_receipt`; one possession proof per class every `REFRESH` DAA rides the main slot
//! when it is free, else the readiness lane (V07: its own confirmed output, one in flight). A carrier lands `land` DAA after it is
//! sent (the next block, or one later), and only then frees its slot. The load values are TEST inputs, not measurements; the
//! scheduler, the slot bound and the window are the shipped ones.
use super::{MAX_INFLIGHT_CARRIERS, PalwCarrierLaneV1, PalwCarrierSiteV1, PalwCarrierSlotsV1};
use std::collections::VecDeque;

const DUTY_EVERY: u64 = 30;
const PANEL: u64 = 7;
const REFRESH: u64 = 120;
const HORIZON: u64 = 6_000;

#[derive(Default, Clone, Copy)]
struct ClassStats {
    worst_wait: u64,
    missed: u64,
    sent: u64,
    /// Missed by kind: possession proofs, licences, own receipts.
    missed_by: [u64; 3],
}

fn note(s: &mut ClassStats, kind: usize, now: u64, entered: u64, deadline: u64, land: u64) {
    s.worst_wait = s.worst_wait.max(now - entered);
    let late = (now + land > deadline) as u64;
    s.missed += late;
    s.missed_by[kind] += late;
    s.sent += 1;
}

/// One run: `classes` classes, carriers landing `land` DAA after they go out, `storm` a court queue that never empties.
fn run(classes: u64, land: u64, window: u64, storm: bool, class_lanes: bool) -> Vec<ClassStats> {
    let mut lane_of: Vec<Option<u64>> = vec![None; classes as usize];
    // Queues per site: (class, entered, deadline).
    let (mut own, mut licences, mut receipts) = (VecDeque::new(), VecDeque::new(), VecDeque::new());
    let mut stats = vec![ClassStats::default(); classes as usize];
    let (mut last, mut landing_at, mut lane_at): (Option<PalwCarrierLaneV1>, Option<u64>, Option<u64>) = (None, None, None);
    for now in 0..HORIZON {
        for c in 0..classes {
            if (now + c) % DUTY_EVERY == 0 {
                let duty = (now + c) / DUTY_EVERY;
                if duty % PANEL == c % PANEL {
                    licences.push_back((c, now, now + window));
                }
                if duty % PANEL == (c + 1) % PANEL {
                    receipts.push_back((c, now, now + window));
                }
            }
            if (now + 3 * c) % REFRESH == 0 {
                own.push_back((c, now, now + REFRESH));
            }
        }
        if landing_at.is_some_and(|at| now >= at) {
            landing_at = None;
        }
        let inflight = landing_at.is_some() as usize;
        let mut slots = PalwCarrierSlotsV1::new(last);
        let mut sent_now = inflight;
        for site in PalwCarrierSiteV1::TICK_ORDER {
            slots.at(site, sent_now);
            if !slots.offers(site, sent_now) {
                continue;
            }
            let queue = match site {
                PalwCarrierSiteV1::Own => Some((0, &mut own)),
                PalwCarrierSiteV1::Licences => Some((1, &mut licences)),
                PalwCarrierSiteV1::OwnReceipts => Some((2, &mut receipts)),
                _ => None,
            };
            let carried = match queue {
                Some((kind, q)) => q.pop_front().map(|(c, entered, deadline)| note(&mut stats[c as usize], kind, now, entered, deadline, land)),
                None => (storm && matches!(site, PalwCarrierSiteV1::PriorityFirst | PalwCarrierSiteV1::PriorityAfterLicences)).then_some(()),
            };
            if carried.is_some() {
                sent_now += 1;
                landing_at = Some(now + land);
            }
        }
        last = slots.finish(sent_now);
        // V07: a possession proof the main slot did not take rides the readiness lane (one in flight on its own output).
        if lane_at.is_some_and(|at| now >= at) {
            lane_at = None;
        }
        if lane_at.is_none()
            && let Some((c, entered, deadline)) = own.pop_front()
        {
            note(&mut stats[c as usize], 0, now, entered, deadline, land);
            lane_at = Some(now + land);
        }
        // The per-class receipt lane (2026-10-18): an own receipt the main slot did not take rides its class's lane — its own
        // confirmed output, one in flight, never a child of another chain's carrier.
        if class_lanes {
            // At most `PALW_RECEIPT_LANES_MAX_V1` lanes per seat (the node's bound): the first classes hold them.
            for c in 0..(classes as usize).min(super::PALW_RECEIPT_LANES_MAX_V1) {
                if lane_of[c].is_some_and(|at| now >= at) {
                    lane_of[c] = None;
                }
                if lane_of[c].is_none()
                    && let Some(i) = receipts.iter().position(|(k, _, _)| *k as usize == c)
                {
                    let (k, entered, deadline) = receipts.remove(i).unwrap();
                    note(&mut stats[k as usize], 2, now, entered, deadline, land);
                    lane_of[c] = Some(now + land);
                }
            }
        }
    }
    // Duties still queued at the horizon past their deadline count as missed.
    for (kind, q) in [(0usize, &own), (1, &licences), (2, &receipts)] {
        for (c, _, deadline) in q.iter() {
            if *deadline < HORIZON {
                stats[*c as usize].missed += 1;
                stats[*c as usize].missed_by[kind] += 1;
            }
        }
    }
    stats
}

#[test]
fn rfc18_iv_one_inflight_carrier_wait_across_many_classes_against_the_receipt_window() {
    assert_eq!(MAX_INFLIGHT_CARRIERS, 1, "the shipped bound under measurement");
    let params = kaspa_consensus_core::config::params::palw_t12_shipped_params();
    let window = match &params.palw_consensus_mode {
        kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(b) => b.state.window_receipt(),
        _ => unreachable!("testnet-12 is ConsensusV2"),
    };
    let mut first_miss: Vec<(bool, bool, u64, u64)> = Vec::new();
    for class_lanes in [false, true] {
    for storm in [false, true] {
        for land in [1u64, 2] {
            for classes in [1u64, 8, 16, 24, 32, 48, 64, 72, 96, 128, 192, 256] {
                let stats = run(classes, land, window, storm, class_lanes);
                let worst = stats.iter().map(|s| s.worst_wait).max().unwrap();
                let missed: u64 = stats.iter().map(|s| s.missed).sum();
                let sent: u64 = stats.iter().map(|s| s.sent).sum();
                let by = stats.iter().fold([0u64; 3], |a, s| [a[0] + s.missed_by[0], a[1] + s.missed_by[1], a[2] + s.missed_by[2]]);
                println!(
                    "RFC-0018 (iv) carriers: class lanes {class_lanes}, storm {storm}, land {land} DAA, {classes} classes: worst wait {worst} DAA \
                     (window_receipt {window}), {missed} missed (proofs {}, licences {}, own receipts {}) of {sent} sent",
                    by[0], by[1], by[2]
                );
                if class_lanes && classes <= super::PALW_RECEIPT_LANES_MAX_V1 as u64 {
                    assert_eq!(by[2], 0, "with per-class lanes no own receipt misses its window (storm {storm}, land {land}, {classes})");
                }
                if missed > 0 && !first_miss.iter().any(|(l, s, n, _)| (*l, *s, *n) == (class_lanes, storm, land)) {
                    first_miss.push((class_lanes, storm, land, classes));
                }
                if classes <= 8 && !storm {
                    assert_eq!(missed, 0, "the genesis catalogue (8 models) misses no window (land {land})");
                }
            }
        }
    }
    }
    println!("RFC-0018 (iv) first window miss per (class lanes, storm, land): {first_miss:?}");
}

#[test]
fn rfc18_iv_a_receipt_lane_takes_one_carrier_in_flight_and_lanes_are_bounded() {
    use super::{PALW_RECEIPT_LANES_MAX_V1, palw_receipt_lane_takes_v1};
    assert!(palw_receipt_lane_takes_v1(Some(0), 3), "a free lane takes the receipt");
    assert!(!palw_receipt_lane_takes_v1(Some(1), 3), "one in flight per lane");
    assert!(palw_receipt_lane_takes_v1(None, PALW_RECEIPT_LANES_MAX_V1 - 1), "a new class opens a lane under the bound");
    assert!(!palw_receipt_lane_takes_v1(None, PALW_RECEIPT_LANES_MAX_V1), "no lane past the bound");
}
