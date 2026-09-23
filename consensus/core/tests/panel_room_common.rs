//! **Panel room (2026-09-24 audit #4 and its review) — shared fixture.** Included by every
//! `panel_room_*` test through `#[path]`; as its own test target it holds no test.
//!
//! The fixture is `dos_l5_common`'s: testnet-12 itself, its bundle's `PalwStateParamsV2`, its own
//! genesis fold, and the extras `palw_transition_extras_for` resolves from the same `Params`. The
//! class gate reads two of the four processor-only extras, so [`room_extras`] fills them as the
//! processor does: `work_target_active` from `Params::palw_work_target` (armed at DAA 0 on
//! testnet-12) and `round_lane` with the execution lane's span.
//!
//! testnet-12's genesis model classes, as its fold writes them: the short-window row
//! (`ebf44d0a…`, window 3 spans, `max_inflight_claims` 5, the review's "8k" row) and the 2M row
//! (`74c67e63…`, window 2,799 spans, `max_inflight_claims` 1). Both are under the held regime
//! (`class_is_held_v1`), both need 7 ready seats, and both open `Prefetching`; a test that needs a
//! row admitting makes it `Active` through the carriage, as `dos_repro_3d` does.
#![allow(dead_code)]

#[path = "dos_l5_common.rs"]
mod dos;
pub use dos::*;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::palw_admission_v2::palw_attempt_derived_pwu_v1;
use kaspa_consensus_core::palw_model_registry_v1::{
    PalwModelLifecycleV1, PalwModelRegistryClassReadV1, PalwSeatReadinessRowV1, palw_model_registry_read_v2,
};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBlockWorkV3, PalwBondKeyV2, PalwChainStateV2, PalwConsensusObjectV2, PalwStateCarriageV2,
    PalwStateDeltaV2, PalwStateParamsV2, PalwStateV2Error, PalwTransitionExtrasV1, apply_palw_transition_v7,
    palw_class_admits_claim_v1, palw_panel_demand_read_v1,
};

/// 10,000,000 MSK: a bond no reservation in these tests comes near.
pub const RICH: u64 = 1_000_000_000_000_000;

/// testnet-12's two genesis model classes: `(short-window row, 2M row)`.
pub fn model_classes(p: &Params) -> (Hash64, Hash64) {
    let classes = genesis_classes(p);
    (classes[1].0, classes[2].0)
}

/// The extras the processor hands the fold at `daa`, as far as the class gate reads them.
pub fn room_extras(p: &Params, daa: u64) -> PalwTransitionExtrasV1 {
    let mut e = extras(p, daa);
    e.work_target_active = p.palw_work_target_at(daa);
    e.round_lane = p.palw_execution_lane_at(daa).map(|lane| kaspa_consensus_core::palw_execution_lane_v1::PalwExecLaneFoldV1 {
        schedule_span_daa: lane.schedule_span_daa_at(daa),
        ..Default::default()
    });
    e
}

/// [`room_extras`] with the 2026-09-23 audit fence forced off: the rule testnet-11 folds under.
pub fn dormant_extras(p: &Params, daa: u64) -> PalwTransitionExtrasV1 {
    let mut e = room_extras(p, daa);
    e.audit_2026_09_23_active = false;
    e.settled_anchor_depth = None;
    e
}

/// One block through the real fold with the given extras.
#[allow(clippy::too_many_arguments)]
pub fn fold_with(
    p: &Params,
    sp: &PalwStateParamsV2,
    parent: &PalwChainStateV2,
    c: &PalwBlockContextV2,
    objects: &[PalwConsensusObjectV2],
    work: PalwBlockWorkV3<'_>,
    key: Hash64,
    e: &PalwTransitionExtrasV1,
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
        e,
    )
}

/// One block through the real fold with [`room_extras`].
pub fn go(
    p: &Params,
    sp: &PalwStateParamsV2,
    parent: &PalwChainStateV2,
    c: &PalwBlockContextV2,
    objects: &[PalwConsensusObjectV2],
    work: PalwBlockWorkV3<'_>,
    key: Hash64,
) -> Result<(PalwChainStateV2, PalwStateDeltaV2, Vec<(Hash64, String)>), PalwStateV2Error> {
    fold_with(p, sp, parent, c, objects, work, key, &room_extras(p, c.daa_score))
}

/// `s` with `edit` applied to its carriage (the load / reorg-rebuild path).
pub fn edited(sp: &PalwStateParamsV2, s: &PalwChainStateV2, edit: impl FnOnce(&mut PalwStateCarriageV2)) -> PalwChainStateV2 {
    let mut c = PalwStateCarriageV2::from_state(s);
    edit(&mut c);
    c.into_state(sp, None).expect("a consistent carriage")
}

/// `class`'s row made `Active` (admitting in full).
pub fn activated(sp: &PalwStateParamsV2, s: &PalwChainStateV2, class: Hash64) -> PalwChainStateV2 {
    edited(sp, s, |c| c.model_lifecycles.get_mut(&class).expect("a model row").state = PalwModelLifecycleV1::Active)
}

/// `bonds` proved ready for `class` at `daa` (testnet-12's span is one DAA).
pub fn readied(sp: &PalwStateParamsV2, s: &PalwChainStateV2, bonds: &[PalwBondKeyV2], class: Hash64, daa: u64) -> PalwChainStateV2 {
    edited(sp, s, |c| {
        for k in bonds {
            c.seat_readiness.insert(
                (*k, class),
                PalwSeatReadinessRowV1 { proved_daa: daa, proved_span: daa, leaf_index: 0, proof_version: 2, chunks: 8 },
            );
        }
    })
}

/// The genesis bonds' keys, in registry order (eight on testnet-12).
pub fn honest(p: &Params) -> Vec<PalwBondKeyV2> {
    genesis_bonds(p).iter().map(|(k, _, _)| *k).collect()
}

/// The first `n` genesis bonds as panel seats.
pub fn honest_seats(p: &Params, n: usize) -> Vec<(PalwBondKeyV2, Hash64)> {
    genesis_bonds(p).iter().take(n).map(|(k, o, _)| (*k, *o)).collect()
}

/// Op 186's reading of `class` at `daa`, with the arguments the node passes past the fence.
pub fn op186(p: &Params, sp: &PalwStateParamsV2, s: &PalwChainStateV2, class: Hash64, daa: u64) -> PalwModelRegistryClassReadV1 {
    let fold = registry_fold(p, daa);
    let enforced = p.palw_work_target_at(daa) && fold.as_ref().is_some_and(|f| f.governs_at(daa));
    let read = palw_model_registry_read_v2(s, sp, daa, Some(0), fold.as_ref(), None, p.palw_audit_2026_09_23_active_at(daa), enforced);
    read.classes.into_iter().find(|c| c.class_id == class).expect("the class is read")
}

/// The fold's own class gate on `s` for one claim of `class` at `daa` (what the producer asks).
pub fn gate(p: &Params, sp: &PalwStateParamsV2, s: &PalwChainStateV2, class: Hash64, daa: u64) -> Result<(), PalwStateV2Error> {
    palw_class_admits_claim_v1(s, sp, &room_extras(p, daa), &class, daa)
}

/// The claims of `class` the rate rule counts as owed on `s` (the read op 186 and the gate share).
pub fn owed(p: &Params, s: &PalwChainStateV2, class: Hash64) -> u128 {
    let b = bundle(p);
    palw_panel_demand_read_v1(s, &b.state, b.panel.seat_count() as u32).0.get(&class).copied().unwrap_or(0)
}

/// The pwu an attempt on `class` carries under testnet-12's canonical work (`canonical_work_daa = 0`).
pub fn class_pwu(p: &Params, s: &PalwChainStateV2, class: Hash64, daa: u64) -> u64 {
    let target = genesis_classes(p).iter().find(|c| c.0 == class).expect("a genesis class").2;
    palw_attempt_derived_pwu_v1(target, s.palw_canonical_per_draw_v1(&class, daa, Some(0)).expect("a model row"))
}

/// `claim` bound to `seats` in one block at `daa`.
pub fn bound(
    p: &Params,
    sp: &PalwStateParamsV2,
    s: &PalwChainStateV2,
    claim: Hash64,
    seats: &[(PalwBondKeyV2, Hash64)],
    daa: u64,
) -> PalwChainStateV2 {
    go(
        p,
        sp,
        s,
        &ctx(0xB0_0000 + daa, daa, daa, 0),
        &[PalwConsensusObjectV2::PanelBound { claim, anchor: h(0xB1_0000 + daa), seats: seats_of(seats) }],
        PalwBlockWorkV3::None,
        Hash64::default(),
    )
    .expect("the panel binds")
    .0
}

/// `claim` licensed by a `Valid` receipt from each of `seats` in one block at `daa`.
pub fn licensed(
    p: &Params,
    sp: &PalwStateParamsV2,
    s: &PalwChainStateV2,
    claim: Hash64,
    seats: &[(PalwBondKeyV2, Hash64)],
    daa: u64,
) -> PalwChainStateV2 {
    let keys: Vec<PalwBondKeyV2> = seats.iter().map(|(k, _)| *k).collect();
    go(
        p,
        sp,
        s,
        &ctx(0xC0_0000 + daa, daa, daa, 0),
        &[PalwConsensusObjectV2::ReceiptLicensed { claim, receipts: valid_receipts(claim, &keys) }],
        PalwBlockWorkV3::None,
        Hash64::default(),
    )
    .expect("the licence folds")
    .0
}
