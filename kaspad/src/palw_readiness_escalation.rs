//! **The seat's escalated possession proof** (the 2026-09-25 model-registry review, M1) — the
//! node-policy half of `kaspa_consensus_core::palw_readiness_escalation_v1` that the panel's tick
//! reads: which of this tick's proofs escalates, and which one takes the `ReadinessEscalated` site
//! of the one carrier scheduler (`palw_panel::PalwCarrierSlotsV1`).
//!
//! A proof escalates when its row at the tip exists and is lapsing or lapsed and the proof renews it
//! (the one predicate the pool and the tip read ask too), past R-core+ only — and not while this
//! seat's own last proof for the class is still landing: a copy sent then cannot land sooner, so
//! hurrying it would only take the court's slot. The scheduler then carries at most one escalated
//! proof a tick — the most urgent — never two slots running, ahead of the court queue, and
//! transparent to P2-6's licence turn; a proof that does not escalate keeps the Own site exactly as
//! before.
//!
//! **What this cannot do** (the M1 review, MEDIUM 3; measured in the tests below): one carrier is
//! in flight per panel (`MAX_INFLIGHT_CARRIERS`) and every carrier chains on the last one's change,
//! so while a seat's proof waits for a template's head its court and licence carriers wait too. With
//! eight seats × two classes sharing one head a block, the seats spend a large share of their slots
//! blocked behind proofs. Lifting the cap for proofs alone would not help — a court carrier chained
//! on a waiting proof's change is mined only after it — and a second funding chain for proofs is a
//! funding change for the operator to decide.

use kaspa_consensus_core::palw_model_registry_v1::{PalwRegistryGlobalsV1, PalwSeatReadinessRowV1};
use kaspa_consensus_core::palw_readiness_escalation_v1::{
    PALW_READINESS_ESCALATION_LANDING_DAA_V1, PalwReadinessUrgencyV1, palw_readiness_proof_urgency_v1,
};
use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;
use kaspa_hashes::Hash64;

/// One possession proof this tick may carry (`PalwPanelService::readiness_duties`), and how urgently
/// it escalates ([`palw_readiness_duty_urgency_v1`]; `None`: it does not).
#[derive(Clone, Debug)]
pub struct PalwReadinessDutyV1 {
    pub object: PalwConsensusObjectV2,
    pub urgency: Option<PalwReadinessUrgencyV1>,
}

impl PalwReadinessDutyV1 {
    /// The class the proof is for.
    pub fn class_id(&self) -> Option<Hash64> {
        match &self.object {
            PalwConsensusObjectV2::SeatReadinessProved { class_id, .. }
            | PalwConsensusObjectV2::SeatReadinessProvedV2 { class_id, .. } => Some(*class_id),
            _ => None,
        }
    }

    /// Whether the proof escalates (takes the `ReadinessEscalated` site).
    pub fn escalates(&self) -> bool {
        self.urgency.is_some()
    }
}

/// **Is this seat's last proof for a class still landing?** — submitted in `last_submitted_span`,
/// and `now_daa` inside the [`PALW_READINESS_ESCALATION_LANDING_DAA_V1`] a carrier takes from there.
/// On five-DAA spans this never outlasts the span the duty already skips.
pub fn palw_readiness_proof_in_flight_v1(last_submitted_span: Option<u64>, now_daa: u64, span_daa: u64) -> bool {
    last_submitted_span
        .is_some_and(|span| now_daa < span.saturating_mul(span_daa.max(1)).saturating_add(PALW_READINESS_ESCALATION_LANDING_DAA_V1))
}

/// **Is this span's proof held back because the seat's last proof for the class is still landing?**
/// Past R-core+ only (`armed`). A copy sent then cannot land sooner — its row is written by the same
/// block either way — so it would only take a slot, and since an escalation keeps the licences' turn
/// (`PalwCarrierLaneV1::Readiness`), the slot right after one is the licences': the copy took it at
/// the Own site (the M1 review, MEDIUM 3, found in the single-seat storm once the turn was kept).
pub fn palw_readiness_duty_waits_v1(armed: bool, last_submitted_span: Option<u64>, now_daa: u64, span_daa: u64) -> bool {
    armed && palw_readiness_proof_in_flight_v1(last_submitted_span, now_daa, span_daa)
}

/// **How urgently does this span's proof escalate?** `armed` (R-core+ in force at `now_daa`), the
/// proof's urgency against the tip's `row` (`palw_readiness_proof_urgency_v1`: a row that exists and
/// is lapsing or lapsed, and a proof that renews it), and this seat's own last proof for the class is
/// not still landing ([`palw_readiness_proof_in_flight_v1`]). `None`: it does not escalate.
#[allow(clippy::too_many_arguments)]
pub fn palw_readiness_duty_urgency_v1(
    armed: bool,
    row: Option<&PalwSeatReadinessRowV1>,
    span_now: u64,
    proof_version: u8,
    last_submitted_span: Option<u64>,
    now_daa: u64,
    span_daa: u64,
    g: &PalwRegistryGlobalsV1,
    readiness_v2: bool,
) -> Option<PalwReadinessUrgencyV1> {
    if !armed || palw_readiness_proof_in_flight_v1(last_submitted_span, now_daa, span_daa) {
        return None;
    }
    palw_readiness_proof_urgency_v1(row, span_now, proof_version, now_daa, span_daa, g, readiness_v2)
}

/// **Which proof takes the escalated site**: the most urgent that escalates (the row closest to
/// lapsing; a lapsed row last), the first of a tie — one a tick.
pub fn palw_escalated_readiness_pick_v1(duties: &[PalwReadinessDutyV1]) -> Option<usize> {
    duties.iter().enumerate().filter_map(|(at, duty)| duty.urgency.map(|urgency| (urgency, at))).min().map(|(_, at)| at)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_panel::{PalwCarrierLaneV1, PalwCarrierSiteV1, PalwCarrierSlotsV1};
    use kaspa_consensus_core::palw_model_registry_v1::{
        PALW_REGISTRY_GLOBALS_V1, palw_readiness_duty_due_v2, palw_readiness_max_age_daa_v1,
    };

    const G: PalwRegistryGlobalsV1 = PALW_REGISTRY_GLOBALS_V1;

    fn row(proved_daa: u64) -> PalwSeatReadinessRowV1 {
        PalwSeatReadinessRowV1 { proved_daa, proved_span: proved_daa, leaf_index: 0, proof_version: 2, chunks: 16 }
    }

    fn is_readiness(lane: Option<PalwCarrierLaneV1>) -> bool {
        matches!(lane, Some(PalwCarrierLaneV1::Readiness { .. }))
    }

    /// **Armed only past R-core+, only for a row that exists and is lapsing or lapsed, never for a
    /// copy of a proof still landing** — and the pick is the most urgent escalating proof, one a tick.
    #[test]
    fn a_duty_escalates_only_past_the_fence_for_a_lapsing_row_not_already_landing() {
        let r = row(100);
        let lapsing = Some(PalwReadinessUrgencyV1::Lapsing { last_fresh_daa: 108 });
        assert_eq!(palw_readiness_duty_urgency_v1(true, Some(&r), 106, 2, Some(100), 106, 1, &G, true), lapsing, "age 6: escalated");
        assert_eq!(palw_readiness_duty_urgency_v1(false, Some(&r), 106, 2, Some(100), 106, 1, &G, true), None, "the twin: never");
        assert_eq!(palw_readiness_duty_urgency_v1(true, Some(&r), 105, 2, Some(100), 105, 1, &G, true), None, "age 5: the Own site");
        assert_eq!(palw_readiness_duty_urgency_v1(true, Some(&r), 107, 2, Some(106), 107, 1, &G, true), None, "its 106 proof lands");
        assert_eq!(palw_readiness_duty_urgency_v1(true, Some(&r), 108, 2, Some(106), 108, 1, &G, true), lapsing, "…and did not");
        assert_eq!(palw_readiness_duty_urgency_v1(true, None, 108, 2, None, 108, 1, &G, true), None, "a first proof: the Own site");
        assert_eq!(
            palw_readiness_duty_urgency_v1(true, Some(&r), 120, 2, Some(108), 120, 1, &G, true),
            Some(PalwReadinessUrgencyV1::Lapsed),
            "a seat back from downtime recovers its row, behind every lapsing one"
        );
        assert!(!palw_readiness_proof_in_flight_v1(Some(10), 52, 5), "five-DAA spans: the next span is past the landing");
        assert!(palw_readiness_duty_waits_v1(true, Some(106), 107, 1), "its proof of 106 is landing: no copy at 107");
        assert!(!palw_readiness_duty_waits_v1(true, Some(106), 108, 1), "…and at 108 it did not land: send again");
        assert!(!palw_readiness_duty_waits_v1(false, Some(106), 107, 1), "the twin: today's duty, unchanged");
        assert!(!palw_readiness_duty_waits_v1(true, None, 107, 1));
        let object = |class: u64| PalwConsensusObjectV2::SeatReadinessProvedV2 {
            bond: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(
                Hash64::from_u64_word(1),
                0,
            )),
            class_id: Hash64::from_u64_word(class),
            span: 1,
            proof: Box::new(kaspa_consensus_core::palw_artifact::PalwArtifactMultiproofV1 {
                leaf_count: 1,
                opened: vec![],
                siblings: vec![],
            }),
            signature: vec![],
        };
        let duties = vec![
            PalwReadinessDutyV1 { object: object(1), urgency: None },
            PalwReadinessDutyV1 { object: object(2), urgency: Some(PalwReadinessUrgencyV1::Lapsed) },
            PalwReadinessDutyV1 { object: object(3), urgency: Some(PalwReadinessUrgencyV1::Lapsing { last_fresh_daa: 110 }) },
            PalwReadinessDutyV1 { object: object(4), urgency: Some(PalwReadinessUrgencyV1::Lapsing { last_fresh_daa: 109 }) },
            PalwReadinessDutyV1 { object: object(5), urgency: Some(PalwReadinessUrgencyV1::Lapsing { last_fresh_daa: 109 }) },
        ];
        assert_eq!(palw_escalated_readiness_pick_v1(&duties), Some(3), "the row closest to lapsing, the first of a tie");
        assert_eq!(palw_escalated_readiness_pick_v1(&duties[..3]), Some(2), "a lapsing row before a lapsed one");
        assert_eq!(duties[1].class_id(), Some(Hash64::from_u64_word(2)));
        assert!(!duties[0].escalates() && duties[1].escalates());
        assert_eq!(palw_escalated_readiness_pick_v1(&duties[..1]), None, "nothing escalates: nothing is picked");
    }

    /// **On testnet-12's horizon the seat's duty and its escalation follow it** (user decision
    /// 2026-09-25, readiness capacity option (a)): the globals testnet-12's processor hands the fold
    /// carry 24 spans, so a row proved at 100 is re-proved past 112 (the Own site, 13 DAA old) and
    /// escalates from 122 (`24 − 2`) through 124, the last DAA it counts; past it the row lapsed —
    /// the same functions, with the horizon as their input. The default globals keep 4 and 106.
    #[test]
    fn on_testnet12s_horizon_the_duty_is_twelve_and_the_escalation_twenty_two() {
        use kaspa_consensus_core::network::{NetworkId, NetworkType};
        let t12 = kaspa_consensus_core::config::params::Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12));
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &t12.palw_consensus_mode else {
            panic!("testnet-12 is ConsensusV2")
        };
        let g = kaspa_consensus_core::palw_model_registry_v1::palw_registry_globals_of_bundle_v1(bundle);
        assert_eq!(g.readiness_v2_max_age_spans, 24, "testnet-12's fold is handed the 24-span horizon");
        assert_eq!(palw_readiness_max_age_daa_v1(1, &g, true), 24);
        let r = row(100);
        for (globals, duty, escalates, last) in [(&G, 4u64, 106u64, 108u64), (&g, 12, 122, 124)] {
            let due = |now: u64| palw_readiness_duty_due_v2(Some(&r), now, now, None, 1, globals, true);
            assert!(!due(100 + duty) && due(100 + duty + 1), "the duty is due past {duty}");
            let urgency = |now: u64| palw_readiness_duty_urgency_v1(true, Some(&r), now, 2, None, now, 1, globals, true);
            assert_eq!(urgency(escalates - 1), None, "{} DAA old: the Own site", escalates - 1 - 100);
            let lapsing = Some(PalwReadinessUrgencyV1::Lapsing { last_fresh_daa: last });
            assert_eq!(urgency(escalates), lapsing, "max − 2: escalated");
            assert_eq!(urgency(last), lapsing, "the last DAA it counts");
            assert_eq!(urgency(last + 1), Some(PalwReadinessUrgencyV1::Lapsed), "past it: lapsed");
        }
    }

    /// **The panel reads these rules, not copies of them**: `readiness_duties` holds a due proof back
    /// while the seat's last one for the class is still landing (`palw_readiness_duty_waits_v1`, past
    /// R-core+) and asks `palw_readiness_duty_urgency_v1` for its escalation, and the tick's
    /// `ReadinessEscalated` site takes `palw_escalated_readiness_pick_v1`'s proof.
    #[test]
    fn the_panel_reads_the_duty_rules_from_here() {
        let source = include_str!("palw_panel.rs");
        let duties = &source[source.find("    fn readiness_duties(").expect("readiness_duties")..];
        let duties = &duties[..duties.find("\n    }\n").expect("its end")];
        let due = duties.find("palw_readiness_duty_due_v2(").expect("the duty");
        let waits = duties.find("|| crate::palw_readiness_escalation::palw_readiness_duty_waits_v1(").expect("the copy guard");
        assert!(due < waits && waits - due < 600, "the guard is the duty's own condition");
        assert!(duties.contains("crate::palw_readiness_escalation::palw_readiness_duty_urgency_v1("));
        assert!(duties.contains("self.consensus_config.params.palw_rcore_plus_active_at(current_daa),\n                last,"));
        assert!(source.contains("crate::palw_readiness_escalation::palw_escalated_readiness_pick_v1(duties)"));
    }

    /// Runs one tick's carrier sites through the tick's own scheduler, in the tick's own order.
    fn carrier_tick(
        last: Option<PalwCarrierLaneV1>,
        holds: impl Fn(PalwCarrierSiteV1) -> bool,
    ) -> (Vec<PalwCarrierSiteV1>, Option<PalwCarrierLaneV1>) {
        let mut slots = PalwCarrierSlotsV1::new(last);
        let (mut inflight, mut sent) = (0usize, Vec::new());
        for site in PalwCarrierSiteV1::TICK_ORDER {
            slots.at(site, inflight);
            if slots.offers(site, inflight) && holds(site) {
                inflight += 1;
                sent.push(site);
            }
        }
        (sent, slots.finish(inflight))
    }

    /// **An escalated proof is transparent to P2-6's turn** (the M1 review, MEDIUM 3): after a court
    /// carrier, an escalated proof takes the slot and the next slot is still the licences' — P, R, L,
    /// never P, R, P; after a licence the proof takes the slot and the court goes next — L, R, P. Never
    /// two escalations running.
    #[test]
    fn an_escalated_proof_keeps_the_licences_turn() {
        use PalwCarrierSiteV1::{Licences, PriorityAfterLicences, PriorityFirst, ReadinessEscalated};
        let storm = |site: PalwCarrierSiteV1| matches!(site, ReadinessEscalated | PriorityFirst | PriorityAfterLicences | Licences);
        let (sent, lane) = carrier_tick(Some(PalwCarrierLaneV1::Priority), storm);
        assert_eq!((sent, lane), (vec![ReadinessEscalated], Some(PalwCarrierLaneV1::Readiness { licence_turn: true })));
        assert_eq!(carrier_tick(lane, storm).0, vec![Licences], "P, R, L");
        let (sent, lane) = carrier_tick(Some(PalwCarrierLaneV1::Licence), storm);
        assert_eq!((sent, lane), (vec![ReadinessEscalated], Some(PalwCarrierLaneV1::Readiness { licence_turn: false })));
        assert_eq!(carrier_tick(lane, storm).0, vec![PriorityFirst], "L, R, P");
        let only_licences = |site: PalwCarrierSiteV1| matches!(site, ReadinessEscalated | Licences);
        assert_eq!(carrier_tick(Some(PalwCarrierLaneV1::Readiness { licence_turn: false }), only_licences).0, vec![Licences]);
        // A turn with nothing sent after an escalation keeps it.
        let (sent, lane) = carrier_tick(Some(PalwCarrierLaneV1::Readiness { licence_turn: true }), |_| false);
        assert_eq!((sent, lane), (vec![], Some(PalwCarrierLaneV1::Readiness { licence_turn: true })));
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Sent {
        Court,
        Licence,
        Proof { class: usize, span: u64 },
    }

    struct Run {
        sent: Vec<(u64, Sent)>,
        lanes: Vec<Option<PalwCarrierLaneV1>>,
        /// `(daa, class)` for every DAA a row counted for nothing.
        lapsed: Vec<(u64, usize)>,
    }

    /// **One seat's tick, one DAA at a time, on testnet-12's clock** (one block a DAA, one-DAA spans)
    /// **with the default eight-DAA rows** — the tightest horizon a network may configure, so the
    /// storm is at its hardest (testnet-12's own 24-DAA rows are `network_at`'s): the tick's own scheduler and site order, the seat's own duty rule and the
    /// escalation, and a carrier's life — sent at `t`, carried by block `t + 1` (which frees the
    /// slot), accepted by block `t + 2` (which writes the row, dated at the span the proof names).
    /// The registry counts the rows a block's parent left, before the block's own objects.
    ///
    /// `proved[c]` is class `c`'s row at DAA 0 (so `vec![0; 2]` is two classes due in the same DAA).
    fn run(ticks: u64, proved: &[u64], armed: bool, court_files: impl Fn(u64) -> bool, licence_waits: impl Fn(u64) -> bool) -> Run {
        let max_age = palw_readiness_max_age_daa_v1(1, &G, true);
        let classes = proved.len();
        let mut rows: Vec<PalwSeatReadinessRowV1> = proved.iter().map(|p| row(*p)).collect();
        let mut landing: Vec<(u64, usize, u64)> = Vec::new();
        let mut last_submitted: Vec<Option<u64>> = vec![None; classes];
        let (mut court, mut last_lane) = (0u64, None);
        let mut out = Run { sent: Vec::new(), lanes: Vec::new(), lapsed: Vec::new() };
        for t in 0..ticks {
            for (class, r) in rows.iter().enumerate() {
                if t.saturating_sub(r.proved_daa) > max_age {
                    out.lapsed.push((t, class));
                }
            }
            landing.retain(|(at, class, span)| {
                if *at == t {
                    rows[*class] = row(*span);
                }
                *at != t
            });
            if court_files(t) {
                court += 1;
            }
            let mut duties: Vec<(usize, Option<PalwReadinessUrgencyV1>)> = (0..classes)
                .filter(|c| palw_readiness_duty_due_v2(Some(&rows[*c]), t, t, last_submitted[*c], 1, &G, true))
                .filter(|c| !palw_readiness_duty_waits_v1(armed, last_submitted[*c], t, 1))
                .map(|c| (c, palw_readiness_duty_urgency_v1(armed, Some(&rows[c]), t, 2, last_submitted[c], t, 1, &G, true)))
                .collect();
            let mut slots = PalwCarrierSlotsV1::new(last_lane);
            let mut inflight = 0usize; // last tick's carrier was carried by this block
            for site in PalwCarrierSiteV1::TICK_ORDER {
                slots.at(site, inflight);
                if !slots.offers(site, inflight) {
                    continue;
                }
                let sent = match site {
                    PalwCarrierSiteV1::ReadinessEscalated => duties
                        .iter()
                        .enumerate()
                        .filter_map(|(at, (_, urgency))| urgency.map(|urgency| (urgency, at)))
                        .min()
                        .map(|(_, at)| Sent::Proof { class: duties.remove(at).0, span: t }),
                    PalwCarrierSiteV1::PriorityFirst | PalwCarrierSiteV1::PriorityAfterLicences => (court > 0).then(|| {
                        court -= 1;
                        Sent::Court
                    }),
                    PalwCarrierSiteV1::Own => (!duties.is_empty()).then(|| Sent::Proof { class: duties.remove(0).0, span: t }),
                    PalwCarrierSiteV1::Licences => licence_waits(t).then_some(Sent::Licence),
                    PalwCarrierSiteV1::OwnReceipts => None,
                };
                if let Some(sent) = sent {
                    inflight += 1;
                    if let Sent::Proof { class, span } = sent {
                        last_submitted[class] = Some(span);
                        landing.push((t + 2, class, span));
                    }
                    out.sent.push((t, sent));
                }
            }
            last_lane = slots.finish(inflight);
            out.lanes.push(last_lane);
        }
        out
    }

    /// **The storm: a court queue that never empties and a licence always waiting**, one seat whose
    /// every carrier lands in the next block. Today's order carries a seat's proofs only on the
    /// licences' turn, so two classes due in the same DAA put the second one's carrier in at age 7 and
    /// its row lapses; escalated, the second goes at age 6 ahead of the court and no row ever lapses —
    /// while the court keeps two slots in five, an escalation never takes two slots running, and it
    /// never takes the licences' turn (MEDIUM 3), so the licences keep exactly the share today's order
    /// gives them (a sixth of the slots with two classes, a third with one).
    #[test]
    fn a_da_storm_never_lapses_a_row_and_the_court_and_licences_still_flow() {
        const TICKS: u64 = 1_000;
        for classes in [1usize, 2] {
            let armed = run(TICKS, &vec![0; classes], true, |_| true, |_| true);
            let today = run(TICKS, &vec![0; classes], false, |_| true, |_| true);
            assert!(
                armed.lapsed.is_empty(),
                "{classes} classes: no row lapses under the storm: {:?}",
                &armed.lapsed[..armed.lapsed.len().min(5)]
            );
            let count = |run: &Run, kind: fn(&Sent) -> bool| run.sent.iter().filter(|(_, s)| kind(s)).count();
            let court = count(&armed, |s| *s == Sent::Court);
            let licences = count(&armed, |s| *s == Sent::Licence);
            assert!(court * 5 >= TICKS as usize * 2, "{classes} classes: the court keeps two slots in five ({court} of {TICKS})");
            assert!(
                licences * 6 >= TICKS as usize - 6,
                "{classes} classes: licences keep a sixth of the slots ({licences} of {TICKS})"
            );
            assert!(
                licences >= count(&today, |s| *s == Sent::Licence),
                "{classes} classes: an escalation never takes the licences' turn: {licences} vs today's {}",
                count(&today, |s| *s == Sent::Licence)
            );
            assert!(
                armed.lanes.windows(2).all(|pair| !(is_readiness(pair[0]) && is_readiness(pair[1]))),
                "an escalation never takes two slots running"
            );
            let proofs = count(&armed, |s| matches!(s, Sent::Proof { .. }));
            let escalated = armed.lanes.iter().filter(|lane| is_readiness(**lane)).count();
            println!(
                "storm, {classes} classes, one seat: court {court}, licences {licences}, proofs {proofs} ({escalated} escalated) of {TICKS} slots"
            );
        }
        let today = run(TICKS, &[0, 0], false, |_| true, |_| true);
        assert!(!today.lapsed.is_empty(), "the hole this closes: today's order lapses a row under the same storm");
    }

    /// **A known P2-6 gap, not M1's** (the M1 review, LOW 2): with three classes due in staggered
    /// DAA on one seat, the Own site — ahead of the collector on the licences' turn, P2-6's own order
    /// — takes nearly every licences' turn under a storm, so the licences starve with or without M1.
    /// M1 never makes it worse (its escalations keep the licences' turn), and with the three due in
    /// the same DAA its copy guard frees them (today's seat re-sent each proof on the licences' turn).
    #[test]
    fn three_classes_on_one_seat_starve_its_licences_with_or_without_m1() {
        const TICKS: u64 = 1_000;
        for proved in [vec![0u64, 0, 0], vec![0, 2, 4]] {
            let armed = run(TICKS, &proved, true, |_| true, |_| true);
            let today = run(TICKS, &proved, false, |_| true, |_| true);
            let licences = |run: &Run| run.sent.iter().filter(|(_, s)| *s == Sent::Licence).count();
            println!("storm, 3 classes {proved:?}: licences armed {}, today {}", licences(&armed), licences(&today));
            assert!(licences(&armed) >= licences(&today), "{proved:?}: M1 never costs the licences a slot");
            assert!(licences(&today) * 20 < TICKS as usize, "{proved:?}: today's P2-6 gap: {}", licences(&today));
            if proved == [0, 2, 4] {
                assert!(licences(&armed) * 20 < TICKS as usize, "staggered: the gap stands with M1: {}", licences(&armed));
            }
        }
    }

    /// **Sparse traffic: nothing is hurried and nothing lapses; the copies go.** A court move every
    /// tenth DAA, no licences: the court carriers go in the same DAA armed or not, no row lapses either
    /// way — and the armed seat no longer sends a second copy of each proof the DAA after the first
    /// (the row is written a block after its carrier is mined, so today's duty found it still due and
    /// re-sent: every proof twice, `palw_readiness_duty_waits_v1`). Past R-core+ that halves the
    /// seats' own demand on the block.
    #[test]
    fn sparse_traffic_is_not_hurried_and_sends_no_copies() {
        for proved in [vec![0u64], vec![0, 3], vec![0, 2, 4]] {
            let armed = run(500, &proved, true, |t| t % 10 == 3, |_| false);
            let today = run(500, &proved, false, |t| t % 10 == 3, |_| false);
            let court = |run: &Run| run.sent.iter().filter(|(_, s)| *s == Sent::Court).copied().collect::<Vec<_>>();
            let proofs = |run: &Run| {
                run.sent
                    .iter()
                    .filter_map(|(t, s)| if let Sent::Proof { class, .. } = s { Some((*t, *class)) } else { None })
                    .collect::<Vec<_>>()
            };
            assert_eq!(court(&armed), court(&today), "{proved:?}: the same court carriers, in the same DAA");
            assert!(armed.lapsed.is_empty() && today.lapsed.is_empty(), "{proved:?}: no row lapses");
            let copies = |proofs: &[(u64, usize)]| proofs.iter().filter(|(t, c)| proofs.contains(&(t.wrapping_sub(1), *c))).count();
            assert_eq!(copies(&proofs(&armed)), 0, "{proved:?}: no copy of a proof still landing");
            assert!(copies(&proofs(&today)) > 0, "{proved:?}: today's seat sends them");
            assert!(proofs(&armed).len() < proofs(&today).len(), "{proved:?}: fewer proofs on the block");
        }
    }

    // ---- The network: many seats, one head a block (the M1 review, MEDIUM 3 and LOW 3) ----

    /// **What a block's lane does** in the network model. `H1_LANE` ML-DSA-87 carriers fill the lane
    /// alone (250,000 / 49,044 on testnet-12); `beside` is how many still fit beside a head proof (a
    /// p50-row A16 proof, 207,968 of the lane: 0; a typical one, 195,508: 1).
    #[derive(Clone, Copy, Debug)]
    enum Lane {
        /// No other traffic: `n` proofs a block, escalated first (most urgent, then earliest), then the
        /// rest; no court traffic.
        Open(usize),
        /// A storm: every seat's court queue never empties and better-paying traffic fills the fee
        /// market, so a proof lands only as the head, with `beside` court carriers. `shipped` is the
        /// shipped rule (the M1 review, HIGH 2): when none fits beside the head and the last block
        /// carried a proof, the waiting court carriers lead and the head does not fit behind them;
        /// otherwise — and always without `shipped` — the head leads.
        Storm { shipped: bool, beside: usize },
        /// Today's order under the storm: no head, the lane is the court carriers', no proof lands.
        Today,
    }

    const H1_LANE: usize = 5;

    #[derive(Debug, Default)]
    struct Network {
        slots: u64,
        court: u64,
        licences: u64,
        proofs: u64,
        /// Seat-ticks a seat sent nothing because its one carrier in flight was a proof still waiting.
        blocked: u64,
        rows: u64,
        lapsed: u64,
        /// Class-DAA with fewer than `seat_count` fresh rows (the class is HELD).
        held: u64,
    }

    impl Network {
        fn fresh_rows_per_daa(&self, ticks: u64) -> f64 {
            (self.rows - self.lapsed) as f64 / ticks as f64
        }
    }

    /// **The network's seats, one DAA at a time, on testnet-12's clock**, each with the tick's own
    /// scheduler, site order, duty rule, copy guard and escalation, and — unlike [`run`] — its one
    /// carrier in flight held until a block carries it (`MAX_INFLIGHT_CARRIERS`, the chained funding).
    /// A block carries the head's proofs (the pool's head order: `PalwReadinessUrgencyV1`, then
    /// arrival), then court carriers up to what the lane leaves (earliest first), and every licence
    /// (the fee market is not the limit modelled here). A proof past the fold's landing window is
    /// evicted by the gate and its seat funds afresh. A carried proof writes its row a DAA later.
    fn network(seats: usize, classes: usize, ticks: u64, armed: bool, lane: Lane) -> Network {
        network_at(&G, seats, classes, ticks, armed, lane)
    }

    /// [`network`] under the globals `g` — the horizon a network's fold is handed (testnet-12's
    /// twenty-four spans, user decision 2026-09-25, readiness capacity option (a)): the rows stand
    /// `g`'s horizon, the seats re-prove at half of it and escalate from `max − 2`, and a class is
    /// HELD below `g`'s seat count.
    fn network_at(g: &PalwRegistryGlobalsV1, seats: usize, classes: usize, ticks: u64, armed: bool, lane: Lane) -> Network {
        #[derive(Clone, Copy, PartialEq, Eq)]
        enum Carrier {
            Court { sent: u64 },
            Licence,
            Proof { class: usize, span: u64, sent: u64 },
        }
        let storm = !matches!(lane, Lane::Open(_));
        let max_age = palw_readiness_max_age_daa_v1(1, g, true);
        let landing_window = kaspa_consensus_core::palw_model_registry_v1::palw_readiness_landing_spans_v1(1);
        // Rows staggered over the half-age cadence (`max_age / 2 + 1`: five at eight spans), so the
        // network is not all due in one DAA.
        let cadence = (max_age / 2 + 1) as usize;
        let mut rows: Vec<Vec<PalwSeatReadinessRowV1>> =
            (0..seats).map(|s| (0..classes).map(|c| row(((s + 3 * c) % cadence) as u64)).collect()).collect();
        let mut last_submitted = vec![vec![None::<u64>; classes]; seats];
        let mut last_lane = vec![None::<PalwCarrierLaneV1>; seats];
        let mut pending = vec![None::<Carrier>; seats];
        let mut landing: Vec<(u64, usize, usize, u64)> = Vec::new();
        let mut last_carried = false;
        let mut out = Network::default();
        for t in 8..8 + ticks {
            // The rows the chain counts at this DAA.
            for c in 0..classes {
                let fresh = (0..seats).filter(|s| t.saturating_sub(rows[*s][c].proved_daa) <= max_age).count();
                out.rows += seats as u64;
                out.lapsed += (seats - fresh) as u64;
                out.held += u64::from(fresh < g.seat_count as usize);
            }
            landing.retain(|(at, s, c, span)| {
                if *at == t {
                    rows[*s][*c] = row(*span);
                }
                *at != t
            });
            // Block t.
            let mut heads: Vec<(Option<PalwReadinessUrgencyV1>, u64, usize)> = Vec::new();
            let mut court: Vec<(u64, usize)> = Vec::new();
            for s in 0..seats {
                match pending[s] {
                    Some(Carrier::Licence) => {
                        out.licences += 1;
                        pending[s] = None;
                    }
                    Some(Carrier::Court { sent }) => court.push((sent, s)),
                    Some(Carrier::Proof { span, .. }) if t > span + landing_window => pending[s] = None,
                    Some(Carrier::Proof { class, span, sent }) => {
                        // The pool's tip read: the row at the tip, this DAA (M1's pools).
                        let urgency = palw_readiness_proof_urgency_v1(Some(&rows[s][class]), span, 2, t, 1, g, true);
                        if urgency.is_some() || matches!(lane, Lane::Open(_)) {
                            heads.push((urgency, sent, s));
                        }
                    }
                    None => {}
                }
            }
            heads.sort_by_key(|(urgency, sent, s)| (urgency.is_none(), *urgency, *sent, *s));
            court.sort();
            let (proofs, court_room) = match lane {
                Lane::Open(n) => (n, H1_LANE),
                Lane::Today => (0, H1_LANE),
                Lane::Storm { shipped, beside } => {
                    let carriers_lead = shipped && beside == 0 && last_carried && !court.is_empty();
                    if heads.is_empty() || carriers_lead { (0, H1_LANE) } else { (1, beside) }
                }
            };
            last_carried = false;
            for (_, _, s) in heads.into_iter().take(proofs) {
                let Some(Carrier::Proof { class, span, .. }) = pending[s] else { unreachable!() };
                landing.push((t + 1, s, class, span));
                pending[s] = None;
                out.proofs += 1;
                last_carried = true;
            }
            for (_, s) in court.into_iter().take(court_room) {
                pending[s] = None;
                out.court += 1;
            }
            // Tick t on every seat.
            for s in 0..seats {
                out.slots += 1;
                if matches!(pending[s], Some(Carrier::Proof { .. })) {
                    out.blocked += 1;
                }
                let mut duties: Vec<(usize, Option<PalwReadinessUrgencyV1>)> = (0..classes)
                    .filter(|c| palw_readiness_duty_due_v2(Some(&rows[s][*c]), t, t, last_submitted[s][*c], 1, g, true))
                    .filter(|c| !palw_readiness_duty_waits_v1(armed, last_submitted[s][*c], t, 1))
                    .map(|c| (c, palw_readiness_duty_urgency_v1(armed, Some(&rows[s][c]), t, 2, last_submitted[s][c], t, 1, g, true)))
                    .collect();
                let mut slots = PalwCarrierSlotsV1::new(last_lane[s]);
                let mut inflight = usize::from(pending[s].is_some());
                for site in PalwCarrierSiteV1::TICK_ORDER {
                    slots.at(site, inflight);
                    if !slots.offers(site, inflight) {
                        continue;
                    }
                    let sent = match site {
                        PalwCarrierSiteV1::ReadinessEscalated => duties
                            .iter()
                            .enumerate()
                            .filter_map(|(at, (_, urgency))| urgency.map(|urgency| (urgency, at)))
                            .min()
                            .map(|(_, at)| duties.remove(at).0)
                            .map(|class| Carrier::Proof { class, span: t, sent: t }),
                        PalwCarrierSiteV1::PriorityFirst | PalwCarrierSiteV1::PriorityAfterLicences => {
                            storm.then_some(Carrier::Court { sent: t })
                        }
                        PalwCarrierSiteV1::Own => {
                            (!duties.is_empty()).then(|| Carrier::Proof { class: duties.remove(0).0, span: t, sent: t })
                        }
                        PalwCarrierSiteV1::Licences => storm.then_some(Carrier::Licence),
                        PalwCarrierSiteV1::OwnReceipts => None,
                    };
                    if let Some(carrier) = sent {
                        if let Carrier::Proof { class, span, .. } = carrier {
                            last_submitted[s][class] = Some(span);
                        }
                        pending[s] = Some(carrier);
                        inflight += 1;
                    }
                }
                last_lane[s] = slots.finish(inflight);
            }
        }
        out
    }

    /// **Eight seats × two classes sharing one head a block** (the M1 review, MEDIUM 3, HIGH 2 and
    /// LOW 3): what the single-seat storm above cannot show. A seat's one carrier in flight is held
    /// while its proof waits for the network's head, so its court carrier and its licence wait too;
    /// the rows kept fresh fall well short of the capacity bound (proofs a block × 6), because the
    /// two-DAA margin leaves no room to queue and sixteen rows re-proved at the half-age cadence ask
    /// 3.2 proofs a block. What the shipped order must still do: (a) with p50-row proofs — no ML-DSA
    /// carrier fits beside the head — keep the court moving, which a head every block does not (the
    /// review's HIGH 2: the lane beside it is empty); (b) keep more rows fresh than today's order,
    /// which lands no proof at all under the storm; (c) never cost the licences a slot against
    /// today's; (d) stay under the capacity bound. The numbers are printed for the operator.
    #[test]
    fn eight_seats_sharing_one_head_a_block() {
        const TICKS: u64 = 1_000;
        let (seats, classes) = (8usize, 2usize);
        let scenarios = [
            ("no other traffic, 2 proofs a block", Lane::Open(2)),
            ("storm, p50 proofs, a head every block (the reviewed commit)", Lane::Storm { shipped: false, beside: 0 }),
            ("storm, p50 proofs, shipped: the lead alternates", Lane::Storm { shipped: true, beside: 0 }),
            ("storm, typical proofs, shipped: a head every block, one carrier beside it", Lane::Storm { shipped: true, beside: 1 }),
            ("storm, today's order", Lane::Today),
        ];
        let mut results = Vec::new();
        for (name, lane) in scenarios {
            let n = network(seats, classes, TICKS, !matches!(lane, Lane::Today), lane);
            println!(
                "{name}: of {} seat-slots court {} ({:.0}%), licences {} ({:.0}%), proofs {}, blocked behind a proof {} ({:.0}%); \
                 fresh rows {:.1} a DAA of {}; lapsed row-DAA {} of {}; class-DAA below {} fresh rows {} of {}",
                n.slots,
                n.court,
                100.0 * n.court as f64 / n.slots as f64,
                n.licences,
                100.0 * n.licences as f64 / n.slots as f64,
                n.proofs,
                n.blocked,
                100.0 * n.blocked as f64 / n.slots as f64,
                n.fresh_rows_per_daa(TICKS),
                seats * classes,
                n.lapsed,
                n.rows,
                G.seat_count,
                n.held,
                TICKS * classes as u64
            );
            results.push(n);
        }
        let [open, p50_every, p50_shipped, typical_shipped, today] = &results[..] else { unreachable!() };
        // (a) p50 proofs: the alternation keeps the court moving at least as well as a head every block.
        assert!(p50_shipped.court >= p50_every.court, "{p50_shipped:?} vs {p50_every:?}");
        // (b), (c), (d).
        let bound = |proofs_a_block: f64| proofs_a_block * (palw_readiness_max_age_daa_v1(1, &G, true) - 2) as f64;
        for (n, proofs_a_block) in [(p50_every, 1.0), (p50_shipped, 1.0), (typical_shipped, 1.0), (open, 2.0)] {
            assert!(n.lapsed < today.lapsed, "more rows than today's order: {n:?}");
            assert!(n.fresh_rows_per_daa(TICKS) <= bound(proofs_a_block), "under the bound: {n:?}");
        }
        for n in [p50_every, p50_shipped, typical_shipped] {
            assert!(n.licences >= today.licences && n.court >= today.court, "nothing lost against today's order: {n:?}");
        }
    }

    // ---- M1's measurement: no drill, the real prover path on the in-tree A16 fixture ----

    /// The carrier `PalwPanelService::build_lifecycle_tx` builds, byte for byte in shape: one
    /// ML-DSA-87 P2PKH input signed over its sighash, one change output to the same script, the
    /// object in a 0x4b payload, the relay minimum fee for its compute mass.
    fn carrier_tx(
        object: &PalwConsensusObjectV2,
        params: &kaspa_consensus_core::config::params::Params,
        kp: &libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair,
    ) -> (kaspa_consensus_core::tx::Transaction, kaspa_consensus_core::tx::UtxoEntry) {
        use kaspa_consensus_core::constants::{MAX_TX_IN_SEQUENCE_NUM, SOMPI_PER_KASPA, TX_VERSION};
        use kaspa_consensus_core::hashing::sighash::{Mldsa87SigHashReusedValuesUnsync, calc_mldsa87_signature_hash};
        use kaspa_consensus_core::hashing::sighash_type::SIG_HASH_ALL;
        use kaspa_consensus_core::mass::MassCalculator;
        use kaspa_consensus_core::palw_lifecycle_objects_v2::{PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2};
        use kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE;
        use kaspa_consensus_core::tx::{
            MutableTransaction, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry,
        };
        use kaspa_txscript::script_builder::ScriptBuilder;
        let payload =
            borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object: object.clone() }).unwrap();
        let spk = kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk(
            &kaspa_hashes::blake2b_512_address_payload(kp.verification_key.as_ref()).as_bytes(),
        );
        let funding = UtxoEntry::new(1_000 * SOMPI_PER_KASPA, spk.clone(), 0, false);
        let outpoint = TransactionOutpoint::new(Hash64::from_u64_word(0xF00D), 0);
        let build = |fee: u64, signature_script: Vec<u8>| {
            let mut input = TransactionInput::new(outpoint, vec![], MAX_TX_IN_SEQUENCE_NUM, 1);
            input.signature_script = signature_script;
            Transaction::new(
                TX_VERSION,
                vec![input],
                vec![TransactionOutput::new(funding.amount - fee, spk.clone())],
                0,
                SUBNETWORK_ID_PALW_LIFECYCLE,
                0,
                payload.clone(),
            )
        };
        let masses = MassCalculator::new(
            params.mass_per_tx_byte,
            params.mass_per_script_pub_key_byte,
            params.mass_per_sig_op,
            params.storage_mass_parameter,
        );
        let dummy = ScriptBuilder::new()
            .add_data(&vec![0u8; kaspa_txscript::MLDSA87_SIG_LEN + 1])
            .and_then(|b| b.add_data(kp.verification_key.as_ref()))
            .map(|b| b.drain())
            .unwrap();
        let fee =
            kaspa_pq_validator_core::relay_fee_for_compute_mass(masses.calc_non_contextual_masses(&build(1, dummy)).compute_mass);
        let mtx = MutableTransaction::with_entries(build(fee, vec![]), vec![funding.clone()]);
        let sighash = calc_mldsa87_signature_hash(&mtx.as_verifiable(), 0, SIG_HASH_ALL, &Mldsa87SigHashReusedValuesUnsync::new());
        let mut sig = libcrux_ml_dsa::ml_dsa_87::sign(
            &kp.signing_key,
            sighash.as_bytes().as_slice(),
            kaspa_txscript::MLDSA87_TX_CONTEXT,
            [0u8; 32],
        )
        .unwrap()
        .as_ref()
        .to_vec();
        sig.push(SIG_HASH_ALL.to_u8());
        let mut tx = mtx.tx;
        tx.inputs[0].signature_script =
            ScriptBuilder::new().add_data(&sig).and_then(|b| b.add_data(kp.verification_key.as_ref())).map(|b| b.drain()).unwrap();
        tx.finalize();
        (tx, funding)
    }

    struct Weighed {
        object_bytes: usize,
        operand_bytes: usize,
        opened: usize,
        siblings: usize,
        tx_bytes: u64,
        compute_mass: u64,
        transient_mass: u64,
        storage_mass: u64,
    }

    fn weigh(
        object: &PalwConsensusObjectV2,
        params: &kaspa_consensus_core::config::params::Params,
        kp: &libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair,
    ) -> Weighed {
        use kaspa_consensus_core::mass::{MassCalculator, transaction_estimated_serialized_size};
        let PalwConsensusObjectV2::SeatReadinessProvedV2 { proof, .. } = object else { panic!("a V2 proof") };
        let (tx, funding) = carrier_tx(object, params, kp);
        let masses = MassCalculator::new(
            params.mass_per_tx_byte,
            params.mass_per_script_pub_key_byte,
            params.mass_per_sig_op,
            params.storage_mass_parameter,
        );
        let non_contextual = masses.calc_non_contextual_masses(&tx);
        let storage = masses
            .calc_contextual_masses(
                &kaspa_consensus_core::tx::MutableTransaction::with_entries(tx.clone(), vec![funding]).as_verifiable(),
            )
            .map_or(0, |m| m.storage_mass);
        Weighed {
            object_bytes: borsh::to_vec(object).unwrap().len(),
            operand_bytes: proof.operand_bytes(),
            opened: proof.opened.len(),
            siblings: proof.siblings.len(),
            tx_bytes: transaction_estimated_serialized_size(&tx),
            compute_mass: non_contextual.compute_mass,
            transient_mass: non_contextual.transient_mass,
            storage_mass: storage,
        }
    }

    /// **M1's input for the consensus-side decision, measured, not assumed** (the 2026-09-25
    /// model-registry review): a real V2 possession proof built by the seat's own prover path on the
    /// in-tree A16 fixture (the drill's `PALW-QWEN25-A16-V5` geometry) — the challenge's draw, the
    /// budget-bounded prefix, the multiproof, both ML-DSA-87 signatures — in the carrier the panel
    /// builds, weighed by the mass calculator under testnet-12's parameters; then the same proof shape
    /// at the shipped class's scale (an inventory of `N` leaves, the shipped A16 row sizes), and from
    /// the consensus constants how many `(bond, class)` rows one block a DAA keeps fresh.
    ///
    /// Run with `--nocapture` to read the table. The asserts pin the arithmetic the table rests on.
    #[test]
    fn the_readiness_proof_is_weighed_against_the_block() {
        use kaspa_consensus_core::config::params::Params;
        use kaspa_consensus_core::network::{NetworkId, NetworkType};
        use kaspa_consensus_core::palw_artifact::{
            PalwArtifactOperandV1, artifact_leaf_v1, palw_artifact_multiproof_v1, verify_artifact_multiproof_v1,
        };
        use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
        use kaspa_consensus_core::palw_model_registry_v1::{
            PALW_READINESS_V2_BUDGET_BYTES_V1, PALW_READINESS_V2_FRAME_BYTES_V1, PALW_READINESS_V2_LEAF_MAX_BYTES_V1,
            PALW_SEAT_READINESS_V2_MLDSA87_CONTEXT, palw_readiness_v2_challenge_seed_v1, palw_readiness_v2_draw_v1,
            palw_readiness_v2_opening_is_the_challenge_v1, palw_seat_readiness_message_v2,
        };
        use kaspa_consensus_core::palw_state_v2::{PALW_OBJECT_CHUNK_MAX_BYTES, PalwBondKeyV2};
        use kaspa_consensus_core::tx::TransactionOutpoint;

        let params = Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12));
        let span_daa = params.palw_execution_lane.as_ref().expect("testnet-12 schedules the lane").schedule_span_daa;
        assert!(params.palw_readiness_v2_at(0) && params.palw_rcore_plus_active_at(0), "testnet-12 arms both from genesis");
        // testnet-12's own globals — the ones its processor hands the fold (the bundle's seat count and
        // the 24-span readiness horizon, user decision 2026-09-25, readiness capacity option (a)).
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(t12_bundle) = &params.palw_consensus_mode else {
            panic!("testnet-12 is ConsensusV2")
        };
        let g = kaspa_consensus_core::palw_model_registry_v1::palw_registry_globals_of_bundle_v1(t12_bundle);
        let max_age = palw_readiness_max_age_daa_v1(span_daa, &g, true);
        let kp = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([0x5E; 32]);
        let bond = PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(0xB0), 0));
        let bond_bytes = borsh::to_vec(&bond).unwrap();
        let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
            params.net.to_string().as_bytes(),
            Some(params.genesis.hash),
        );
        let span = 11u64;

        // 1. The in-tree A16 fixture, through the seat's own prover path (`readiness_duties`).
        let geometry = kaspa_consensus_core::palw_e2e_adjudicability::PALW_RC_A16_DRILL_GEOMETRY;
        let shape = misaka_palw_base0::artifact::Base0ShapeV1 {
            n_layers: geometry.layer_count as usize,
            n_heads: geometry.attn_heads as usize,
            n_kv_heads: geometry.attn_kv_heads as usize,
            d_head: geometry.attn_head_dim as usize,
            d_ff: geometry.ffn_dim as usize,
            vocab: geometry.vocab_size as usize,
            max_position: geometry.n_ctx as usize,
            ln_theta_gen_q: misaka_palw_base0::artifact::LN_THETA_10000_GEN_Q,
            eps_q: kaspa_consensus_core::palw_qwen25_profile::QWEN25_A16_ARTIFACT_EPS_Q,
        };
        let artifact = misaka_palw_base0::artifact::Base0ArtifactV1::derive_deterministic(shape, 0x5A16)
            .unwrap()
            .with_a16_params(misaka_palw_base0::engine_a16::derived_a16_store(&shape))
            .unwrap();
        let profile = kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_artifact_row_profile_v5(geometry).unwrap();
        let class_id = profile.shape_profile_id();
        let backend = misaka_palw_base0::qwen25_a16_backend::Qwen25A16Backend::from_registered_profile(
            std::sync::Arc::new(artifact),
            b"misaka-palw-rc".to_vec(),
            profile,
            (4, 2),
        )
        .unwrap();
        let (root, leaf_count) = backend.artifact_root_and_leaf_count().unwrap();
        let seed = palw_readiness_v2_challenge_seed_v1(&class_id, &bond_bytes, span);
        let draw = palw_readiness_v2_draw_v1(&seed, leaf_count);
        let (_, leaves, drawn) = backend.artifact_readiness_material(&draw).unwrap();
        let (mut opened, mut bytes) = (Vec::new(), 0usize);
        for (index, operand) in drawn.clone() {
            if bytes >= PALW_READINESS_V2_BUDGET_BYTES_V1 {
                break;
            }
            assert!(operand.bytes.len() <= PALW_READINESS_V2_LEAF_MAX_BYTES_V1);
            bytes += operand.bytes.len();
            opened.push((index, operand));
        }
        palw_readiness_v2_opening_is_the_challenge_v1(&draw, &opened.iter().map(|(i, o)| (*i, o.bytes.len())).collect::<Vec<_>>())
            .unwrap();
        opened.sort_by_key(|(index, _)| *index);
        let signed = |class_id: Hash64, proof: kaspa_consensus_core::palw_artifact::PalwArtifactMultiproofV1| {
            let message = palw_seat_readiness_message_v2(domain, &bond_bytes, &class_id, span, &proof);
            let signature = libcrux_ml_dsa::ml_dsa_87::sign(
                &kp.signing_key,
                message.as_byte_slice(),
                PALW_SEAT_READINESS_V2_MLDSA87_CONTEXT,
                [0u8; 32],
            )
            .unwrap()
            .as_ref()
            .to_vec();
            PalwConsensusObjectV2::SeatReadinessProvedV2 { bond, class_id, span, proof: Box::new(proof), signature }
        };
        let proof = palw_artifact_multiproof_v1(&leaves, &opened).expect("the seat holds the artifact");
        verify_artifact_multiproof_v1(&proof, root).expect("the proof opens the registered root");
        let fixture = weigh(&signed(class_id, proof), &params, &kp);
        let overhead = fixture.tx_bytes as usize - fixture.object_bytes;
        println!(
            "testnet-12: span {span_daa} DAA, row age {max_age} DAA, block mass {}, target {} ms a block",
            params.max_block_mass,
            params.target_time_per_block()
        );
        println!(
            "A16 fixture ({leaf_count} leaves): {} of {} leaves opened, {} operand bytes, {} siblings; object {} B (frame {} B); \
             carrier tx {} B (overhead {} B); compute mass {}, transient mass {}, storage mass {}",
            fixture.opened,
            draw.len(),
            fixture.operand_bytes,
            fixture.siblings,
            fixture.object_bytes,
            fixture.object_bytes - fixture.operand_bytes,
            fixture.tx_bytes,
            overhead,
            fixture.compute_mass,
            fixture.transient_mass,
            fixture.storage_mass
        );
        assert_eq!(fixture.transient_mass, fixture.tx_bytes * kaspa_consensus_core::constants::TRANSIENT_BYTE_TO_MASS_FACTOR);

        // 2. The same proof at the shipped class's scale: an inventory of `n` leaves, the drawn leaves
        //    at the shipped A16 row sizes (mean 4,204 B: the prefix stops at the 7th; the worst
        //    case: the budget less one byte, then the largest row, 35,840 B).
        let names: Vec<_> = drawn.iter().map(|(_, o)| (o.tensor_name.clone(), o.layer)).collect();
        let shaped = |n: u32, sizes: &[usize]| {
            let draw = palw_readiness_v2_draw_v1(&seed, n);
            let mut leaves: Vec<Hash64> = (0..n as u64).map(Hash64::from_u64_word).collect();
            let mut opened = Vec::new();
            for (k, (index, size)) in draw.iter().zip(sizes).enumerate() {
                let (tensor_name, layer) = names[k % names.len()].clone();
                let operand = PalwArtifactOperandV1 { tensor_name, layer, row_start: *index, bytes: vec![k as u8; *size] };
                leaves[*index as usize] = artifact_leaf_v1(&operand);
                opened.push((*index, operand));
            }
            palw_readiness_v2_opening_is_the_challenge_v1(&draw, &opened.iter().map(|(i, o)| (*i, o.bytes.len())).collect::<Vec<_>>())
                .expect("the prefix the prover would open");
            opened.sort_by_key(|(index, _)| *index);
            let proof = palw_artifact_multiproof_v1(&leaves, &opened).unwrap();
            weigh(&signed(class_id, proof), &params, &kp)
        };
        let small = vec![1_536usize; 16]; // p50 rows: all sixteen open, under the budget
        let typical = vec![4_204usize; 7];
        let worst = vec![PALW_READINESS_V2_BUDGET_BYTES_V1 - 1, 35_840];
        let mut at_shipped_scale = None;
        for log in [16u32, 18, 19, 20] {
            let n = 1u32 << log;
            let (sm, t, w) = (shaped(n, &small), shaped(n, &typical), shaped(n, &worst));
            println!(
                "A16-shaped, 2^{log} leaves: p50 rows {} B operands ({} opened) -> object {} B (frame {} B), tx {} B, transient \
                 mass {}; typical {} B operands -> object {} B (frame {} B), tx {} B, transient mass {}; \
                 worst {} B operands -> tx {} B, transient mass {}",
                sm.operand_bytes,
                sm.opened,
                sm.object_bytes,
                sm.object_bytes - sm.operand_bytes,
                sm.tx_bytes,
                sm.transient_mass,
                t.operand_bytes,
                t.object_bytes,
                t.object_bytes - t.operand_bytes,
                t.tx_bytes,
                t.transient_mass,
                w.operand_bytes,
                w.tx_bytes,
                w.transient_mass
            );
            assert!(w.object_bytes <= PALW_OBJECT_CHUNK_MAX_BYTES, "the worst proof still rides one carrier");
            assert!(t.object_bytes - t.operand_bytes <= PALW_READINESS_V2_FRAME_BYTES_V1, "the frame is inside its allowance");
            if log == 19 {
                at_shipped_scale = Some((sm, t, w));
            }
        }
        let (small, typical, worst) = at_shipped_scale.unwrap();

        // 3. Rows one block a DAA keeps fresh — UPPER BOUNDS (the M1 review, LOW 3): a perfect
        //    schedule, every proof landing in the very next block. A row stands `max_age`; the seat
        //    re-proves it at half that (every `max_age / 2 + 1` DAA), and at the latest it must send
        //    by `max_age − landing`. The realized figures (the seats' own cadence, one carrier in
        //    flight, a queue at the head) come from the network model after the table.
        let block = params.max_block_mass;
        let lane = block / 2; // P2-9's carrier lane: half a block
        let per_block = |w: &Weighed| block / w.transient_mass.max(w.compute_mass);
        let (cadence, latest) = (max_age / 2 + 1, max_age - PALW_READINESS_ESCALATION_LANDING_DAA_V1);
        let (bonds, classes) = (
            kaspa_consensus_core::config::params::PALW_T12_GENESIS_BONDS.len(),
            kaspa_consensus_core::config::params::PALW_T12_GENESIS_HELD_ROWS.len(),
        );
        println!(
            "testnet-12 demand: {bonds} genesis bonds x {classes} held classes = {} rows; a class needs {} fresh rows to stay out of \
             HELD and {} to leave it — {} and {} rows across the classes",
            bonds * classes,
            g.seat_count,
            g.seat_count + g.spare_seats,
            g.seat_count as usize * classes,
            (g.seat_count + g.spare_seats) as usize * classes
        );
        // What the storm's ML-DSA-87 carriers weigh (an accusation's carrier), for the head's rule.
        let accusation = PalwConsensusObjectV2::DefaultAccused {
            claim: Hash64::from_u64_word(0xDA),
            missing_event_index: 0,
            accuser: bond,
            signature: vec![0; kaspa_txscript::MLDSA87_SIG_LEN],
        };
        let accusation_mass =
            kaspa_consensus_core::mass::transaction_estimated_serialized_size(&carrier_tx(&accusation, &params, &kp).0)
                * kaspa_consensus_core::constants::TRANSIENT_BYTE_TO_MASS_FACTOR;
        for (name, w) in [("fixture", &fixture), ("A16 p50 rows", &small), ("A16 typical", &typical), ("A16 worst", &worst)] {
            let p = per_block(w);
            // The shipped head: every block while an ML-DSA carrier still fits beside it, every other
            // block under contention when none does, never when the proof exceeds the lane.
            let heads_per_two_blocks = if w.transient_mass > lane {
                0
            } else if lane - w.transient_mass >= accusation_mass {
                2
            } else {
                1
            };
            let max_proof_tx = PALW_OBJECT_CHUNK_MAX_BYTES as u64 + overhead as u64;
            println!(
                "{name}: {p} proofs a block alone -> at most {} rows kept fresh at the seat's cadence ({cadence} DAA), at most {} \
                 at the latest ({latest} DAA); under a DA storm with better-paying traffic: {} head(s) in two blocks -> at most {} \
                 rows; the largest proof one carrier may hold ({max_proof_tx} B tx) is {} transient mass",
                p * cadence,
                p * latest,
                heads_per_two_blocks,
                heads_per_two_blocks * latest / 2,
                max_proof_tx * kaspa_consensus_core::constants::TRANSIENT_BYTE_TO_MASS_FACTOR
            );
        }
        // What the storm keeps of the lane while a proof leads it.
        for (name, w) in [("A16 p50 rows", &small), ("A16 typical", &typical)] {
            let left = lane - w.transient_mass;
            println!(
                "{name} at the lane's head leaves {left} of the lane's {lane}: {} DA accusation carriers of {accusation_mass} mass \
                 (the lane alone holds {}){}",
                left / accusation_mass,
                lane / accusation_mass,
                if left < accusation_mass { " — so under contention the lead alternates: 2.5 carriers a block" } else { "" }
            );
        }
        // The realized figures (the network model: the seats' own cadence and copy guard, one carrier
        // in flight per seat, the head's queue), 8 seats × 2 classes over 1,000 DAA — at testnet-12's
        // horizon, and beside it at the default eight spans the user moved it from.
        let needed = g.seat_count as usize * classes;
        let mut realized = Vec::new();
        for (name, lane) in [
            ("no other traffic, 2 proofs a block", Lane::Open(2)),
            ("a DA storm, p50 proofs (the lead alternates)", Lane::Storm { shipped: true, beside: 0 }),
            ("a DA storm, typical proofs (a head every block)", Lane::Storm { shipped: true, beside: 1 }),
        ] {
            let n = network_at(&g, bonds, classes, 1_000, true, lane);
            let eight = network_at(&G, bonds, classes, 1_000, true, lane);
            println!(
                "realized, {name}: {:.1} fresh rows a DAA of {} at {max_age} DAA (at least {needed} keep both classes out of HELD); a \
                 class HELD in {} of {} class-DAA — at {} DAA: {:.1} fresh, HELD in {}",
                n.fresh_rows_per_daa(1_000),
                bonds * classes,
                n.held,
                1_000 * classes,
                palw_readiness_max_age_daa_v1(span_daa, &G, true),
                eight.fresh_rows_per_daa(1_000),
                eight.held
            );
            realized.push((n, eight));
        }
        println!(
            "the staleness horizon buys queueing slack as well as capacity: at {max_age} DAA the seats' cadence asks {:.1} proofs a \
             block of {} rows against {} a block, so the head queues and every DAA a proof waits is a DAA its row does not count",
            (bonds * classes) as f64 / cadence as f64,
            bonds * classes,
            per_block(&typical)
        );
        assert_eq!((cadence, latest), (13, 22), "testnet-12's clock: 24-DAA rows, re-proved past 12, escalated from 22");
        // What the horizon bought (the user's option (a)): in every lane at least as many fresh rows and
        // no more HELD class-DAA than at eight spans, and on an open lane both classes never HELD.
        for (n, eight) in &realized {
            assert!(n.fresh_rows_per_daa(1_000) >= eight.fresh_rows_per_daa(1_000) && n.held <= eight.held, "{n:?} vs {eight:?}");
        }
        assert_eq!(realized[0].0.held, 0, "no other traffic: both genesis classes stay out of HELD at 24 DAA");
        assert!(realized[0].0.fresh_rows_per_daa(1_000) >= needed as f64, "and at least {needed} rows fresh a DAA");
        assert_eq!(per_block(&typical), 2, "two typical A16 proofs a block, and no third");
        assert!(typical.transient_mass <= lane, "a typical A16 proof fits the lane's head");
        assert!(worst.transient_mass > lane, "a worst-span A16 proof does not: it rides the fee market even escalated");
    }
}
