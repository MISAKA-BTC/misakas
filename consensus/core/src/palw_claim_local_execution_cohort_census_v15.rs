//! Rebuildable UNHASHED same-root membership census. Data identity only.
//! No source payer scans global F history or owns a complete world copy.
use super::palw_claim_local_rewards_v1 as rewards;
use super::*;
use crate::palw_claim_local_v1::{
    PALW_CLAIM_LOCAL_TABLE_V1, PalwClaimLiabilityFrameV1, PalwClaimLocalRowOwnerV1 as Owner,
};
use crate::palw_native_resource_v1::{
    BTreeNodeEnvelopeV1, NativeCoverageV1, NativeHeapV1, TirNativeRequestsV1,
    reserve_native_requests_v1,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Clock {
    blue: u64,
    daa: u64,
    block: Hash64,
}
impl Clock {
    fn of(p: PalwBlockContextV2) -> Self {
        Self {
            blue: p.blue_score,
            daa: p.daa_score,
            block: p.block,
        }
    }
    fn before(self, p: PalwBlockContextV2) -> bool {
        self.blue <= p.blue_score && self.daa <= p.daa_score
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Stored {
    block: Hash64,
    daa: u64,
    blue: u64,
}
impl Stored {
    fn of(c: &PalwClaimStateV2) -> Self {
        Self {
            block: c.accepted_block,
            daa: c.accepted_daa,
            blue: c.accepted_blue_score,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Key {
    root: Hash64,
    id: Hash64,
    birth: Stored,
}
impl Key {
    fn of(id: Hash64, c: &PalwClaimStateV2) -> Self {
        Self {
            root: c.execution_root,
            id,
            birth: Stored::of(c),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Interval {
    start: Clock,
    inclusive: bool,
    death: Option<Clock>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Member {
    // ONE actual native Claim observation. Never rewritten to fabricate an
    // original or a phase; semantic owners resolve status independently.
    row: PalwClaimStateV2,
    actual_birth: Option<PalwBlockContextV2>,
    scoped_birth: bool,
    current: bool,
    interval: Option<Interval>,
    ambiguous: bool,
    s1_body: Option<Hash64>,
    a5_body: Option<Hash64>,
    owned: bool,
    canonical_rank: u8,
    native_birth: Option<Clock>,
}
#[derive(Clone, Debug, Default)]
pub(super) struct ExecutionCohortCensusV15 {
    members: BTreeMap<Key, Member>,
    compact_by_root: BTreeMap<(Hash64,Hash64),CompactRootSlotV15>,
    compact_complete: bool,
    // Existing-F observations only. Fixed at global bootstrap, no new phase
    // references are appended for each ordinary future full frame.
    observations: BTreeSet<(Key, Clock)>,
    frames: BTreeSet<Clock>,
    bootstrap_frames: BTreeSet<Clock>,
    needs_global_rebuild: bool,
    boundary: Option<PalwBlockContextV2>,
    ready: bool,
    tracking_active: bool,
    legacy_base_present: bool,
    residency: Option<std::sync::Arc<CensusResidencyV15>>,
}
// Cache contents and coverage remain observable to native state equality.
// The residency Arc owns a memory reservation, not consensus/cache data. Ignore
// only that ticket, including its allocation identity and Some/None presence.
// Exhaustive patterns force future cache fields to receive an equality policy.
impl PartialEq for ExecutionCohortCensusV15 {
    fn eq(&self, other: &Self) -> bool {
        let Self { members: a_members, compact_by_root: a_compact_by_root, compact_complete: a_compact_complete, observations: a_observations, frames: a_frames, bootstrap_frames: a_bootstrap_frames, needs_global_rebuild: a_needs_global_rebuild, boundary: a_boundary, ready: a_ready, tracking_active: a_tracking_active, legacy_base_present: a_legacy_base_present, residency: _ } = self;
        let Self { members: b_members, compact_by_root: b_compact_by_root, compact_complete: b_compact_complete, observations: b_observations, frames: b_frames, bootstrap_frames: b_bootstrap_frames, needs_global_rebuild: b_needs_global_rebuild, boundary: b_boundary, ready: b_ready, tracking_active: b_tracking_active, legacy_base_present: b_legacy_base_present, residency: _ } = other;
        (a_members, a_compact_by_root, a_compact_complete, a_observations, a_frames, a_bootstrap_frames, a_needs_global_rebuild, a_boundary, a_ready, a_tracking_active, a_legacy_base_present) == (b_members, b_compact_by_root, b_compact_complete, b_observations, b_frames, b_bootstrap_frames, b_needs_global_rebuild, b_boundary, b_ready, b_tracking_active, b_legacy_base_present)
    }
}
impl Eq for ExecutionCohortCensusV15 {}
struct CensusResidencyV15 {
    _lease: Option<crate::palw_native_resource_v1::NativeDemandLeaseV1>,
}
impl std::fmt::Debug for CensusResidencyV15 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CensusResidencyV15")
    }
}
impl ExecutionCohortCensusV15 {
    pub(super) fn genesis_v15() -> Self {
        Self {
            ready: true,
            compact_complete: true,
            legacy_base_present: true,
            ..Self::default()
        }
    }
}
impl NativeHeapV1 for ExecutionCohortCensusV15 {
    fn native_heap_v1(&self, component: &str) -> NativeCoverageV1 {
        let mut q = TirNativeRequestsV1::new();
        let result = (|| {
            q.exact_v1::<BTreeNodeEnvelopeV1<Key, Member>>(self.members.len() as u64)?;
            q.exact_v1::<BTreeNodeEnvelopeV1<(Hash64,Hash64),CompactRootSlotV15>>(self.compact_by_root.len() as u64)?;
            q.exact_v1::<BTreeNodeEnvelopeV1<(Key, Clock), ()>>(self.observations.len() as u64)?;
            q.exact_v1::<BTreeNodeEnvelopeV1<Clock, ()>>(
                (self.frames.len() + self.bootstrap_frames.len()) as u64,
            )?;
            for m in self.members.values() {
                let h = m.row.native_heap_v1(component);
                q.add_v1(h.bytes().ok_or(h)?)?;
            }
            Ok::<_, NativeCoverageV1>(q.bytes_v1())
        })();
        match result {
            Ok(bytes) => NativeCoverageV1::Bounded { bytes },
            Err(e) => e,
        }
    }
}
fn bad(s: &'static str) -> PalwStateV2Error {
    PalwStateV2Error::DeltaMismatch(s)
}
fn same_identity(a: &PalwClaimStateV2, b: &PalwClaimStateV2) -> bool {
    Stored::of(a) == Stored::of(b)
        && a.execution_root == b.execution_root
        && a.class_id == b.class_id
        && a.bond == b.bond
        && a.trace_root == b.trace_root
        && a.output_root == b.output_root
        && (matches!(
            (&a.source, &b.source),
            (PalwClaimSourceV2::Attempt, PalwClaimSourceV2::Attempt)
        ) || matches!((&a.source,&b.source),(PalwClaimSourceV2::FreePrompt{quanta:a,..},PalwClaimSourceV2::FreePrompt{quanta:b,..})if a==b))
}
#[derive(Clone, Copy)]
enum Seen {
    Current,
    Frame(Clock),
    Scoped(PalwBlockContextV2),
    S1(PalwBlockContextV2, Hash64),
    A5(PalwBlockContextV2, Hash64),
    Owned,
    NativeBirth(PalwBlockContextV2),
}
fn all_candidates(
    s: &PalwChainStateV2,
    p: &PalwStateParamsV2,
    mut visit: impl FnMut(Hash64, &PalwClaimStateV2, Seen) -> Result<(), PalwStateV2Error>,
) -> Result<(), PalwStateV2Error> {
    for (id, c) in &s.claims {
        visit(*id, c, Seen::Current)?
    }
    for (key, raw) in rewards::frames(s) {
        let f = rewards::decoded::<PalwClaimLiabilityFrameV1>(raw)?;
        if key.as_slice()
            != crate::palw_claim_local_v1::palw_claim_liability_frame_key_v1(f.point.block)
            || s.last_point.is_none_or(|tip| {
                f.point.daa_score > tip.daa_score || f.point.blue_score > tip.blue_score
            })
        {
            return Err(bad("cohort census frame key/clock differs"));
        }
        let at = Seen::Frame(Clock::of(f.point));
        rewards::frame_read(&f, |prior, current| {
            for (id, c) in prior.claims.iter().chain(current.claims.iter()) {
                visit(*id, c, at)?
            }
            Ok(())
        })?;
        // Native prefix is essential for insert->void->remove in one block.
        super::palw_claim_local_native_frame_events_v10::visit_rooted_native_events_v10(
            s,
            f.point.block,
            |_, _, e| {
                if e.owner == Owner::ChainClaims {
                    let id = Hash64::from_bytes(
                        e.key
                            .bytes
                            .try_into()
                            .map_err(|_| bad("cohort census claim event key width"))?,
                    );
                    if let Some(raw) = e.new {
                        let c = rewards::decoded::<PalwClaimStateV2>(raw.bytes)?;
                        let seen = if e.old.is_none()
                            && c.accepted_daa == f.point.daa_score
                            && c.accepted_blue_score == f.point.blue_score
                        {
                            Seen::NativeBirth(f.point)
                        } else {
                            at
                        };
                        visit(id, &c, seen)?;
                    }
                }
                Ok(())
            },
        )?;
    }
    super::palw_claim_local_source_admission_v8::visit_retained_admissions_v8(s, |id, view| {
        visit(
            id,
            view.original_claim(),
            Seen::Scoped(view.accepting_point()),
        )
    })?;
    super::palw_claim_local_execution_cohort_archive_v15::visit_current_cohort_members_v15(
        s,
        |root, id, c| {
            if root != c.execution_root {
                return Err(bad(
                    "rooted cohort source changes actual execution identity",
                ));
            }
            // Actual accepting point is deliberately NOT fabricated from STORED
            // carrying provenance. Per-root completeness is checked separately.
            visit(id, c, Seen::Owned)
        },
    )?;
    if let Some(route) = s.kernel_route() {
        for ((table, key), raw) in &route.aux {
            if *table == PALW_CLAIM_LOCAL_TABLE_V1 && key.len() == 65 && key[0] == b'S' {
                let id = Hash64::from_bytes(
                    key[1..]
                        .try_into()
                        .map_err(|_| bad("cohort census S1 key width"))?,
                );
                let row = super::palw_claim_local_compact_sources_v1::read(s, id)?
                    .ok_or(bad("cohort census S1 disappeared"))?;
                visit(
                    id,
                    &row.original,
                    Seen::S1(
                        row.birth,
                        crate::palw_claim_local_v1::palw_liability_material_id_v1(raw),
                    ),
                )?;
            }
        }
    }
    use crate::palw_claim_local_context_nodes_v2::{self as nodes, ComponentSpecV2};
    let reader = super::palw_claim_local_context_store_v2::StateNodesV2(s);
    let spec = ComponentSpecV2 {
        owner: 6,
        table: 1,
        schema: 5,
        key_bytes: 64,
        value_bytes: u64::MAX,
    };
    let mut callback_error = None;
    nodes::complete_rows(
        &reader,
        reader
            .current(spec)
            .map_err(super::palw_claim_local_paid_archive_v2::state_error)?,
        |_, rv| {
            let body = reader.body(rv)?;
            let raw = rewards::decoded::<
                super::palw_claim_local_compact_sources_v1::PalwClosedClaimSourceV1,
            >(&body)
            .map_err(|e| {
                callback_error = Some(e);
                nodes::ContextErrorV2::Malformed("cohort A5 decode")
            })?;
            let checked =
                super::palw_claim_local_exact_closed_source_v5::read_exact_closed_source_v5(
                    s, p, raw.claim, &raw.birth,
                )
                .map_err(|e| {
                    callback_error = Some(e);
                    nodes::ContextErrorV2::Malformed("cohort A5 source")
                })?
                .ok_or(nodes::ContextErrorV2::Unavailable(
                    "cohort A5 actual source absent",
                ))?;
            visit(
                checked.claim,
                &checked.original,
                Seen::A5(checked.birth, rv.data),
            )
            .map_err(|e| {
                callback_error = Some(e);
                nodes::ContextErrorV2::Malformed("cohort A5 callback")
            })
        },
    )
    .map_err(|e| {
        callback_error.unwrap_or_else(|| super::palw_claim_local_paid_archive_v2::state_error(e))
    })?;
    Ok(())
}
fn add_candidate(
    census: &mut ExecutionCohortCensusV15,
    id: Hash64,
    c: &PalwClaimStateV2,
    seen: Seen,
) {
    if let Seen::S1(point,body)=seen {
        census.compact_by_root.insert((c.execution_root,id),Some(CompactRootPointerV15{point,body}));
    }
    if c.execution_root == Hash64::default() {
        return;
    }
    let key = Key::of(id, c);
    let m = census.members.entry(key).or_insert_with(|| Member {
        row: c.clone(),
        actual_birth: None,
        scoped_birth: false,
        current: false,
        interval: None,
        ambiguous: false,
        s1_body: None,
        a5_body: None,
        owned: false,
        canonical_rank: 9,
        native_birth: None,
    });
    let rank = match seen {
        Seen::Owned => 0,
        Seen::Scoped(_) => 1,
        Seen::NativeBirth(_) => 2,
        Seen::S1(..) | Seen::A5(..) => 3,
        _ => 9,
    };
    if !same_identity(&m.row, c) {
        m.ambiguous = true;
    }
    if rank < 9 && rank == m.canonical_rank && m.row != *c {
        m.ambiguous = true;
    }
    if rank < m.canonical_rank {
        m.row = c.clone();
        m.canonical_rank = rank;
    }
    if !same_identity(&m.row, c) {
        m.ambiguous = true;
    }
    let actual = match seen {
        Seen::Scoped(p) | Seen::S1(p, _) | Seen::A5(p, _) | Seen::NativeBirth(p) => Some(p),
        _ => None,
    };
    if let Some(p) = actual {
        if m.actual_birth.is_some_and(|old| old != p) {
            m.ambiguous = true
        } else {
            m.actual_birth = Some(p)
        }
    }
    match seen {
        Seen::Current => {
            m.current = true;
            if let Some(p) = census.boundary {
                m.interval = Some(Interval {
                    start: Clock::of(p),
                    inclusive: false,
                    death: None,
                })
            }
        }
        Seen::Frame(at) => {
            census.observations.insert((key, at));
        }
        Seen::Scoped(_) => m.scoped_birth = true,
        Seen::S1(_, body) => m.s1_body = Some(body),
        Seen::A5(_, body) => m.a5_body = Some(body),
        Seen::Owned => m.owned = true,
        Seen::NativeBirth(p) => {
            m.native_birth = Some(Clock::of(p));
            census.observations.insert((key, Clock::of(p)));
        }
    }
}
/// Native global boundary ONLY. Two source-only traversals count before ONE
/// canonical row/member and observation/index nodes allocate; no world retained.
pub(super) fn rebuild_v15(
    s: &mut PalwChainStateV2,
    p: &PalwStateParamsV2,
) -> Result<(), PalwStateV2Error> {
    if !s
        .last_point
        .is_some_and(|point| p.claim_local_settlement_active_at(point.daa_score))
    {
        s.execution_cohort_census_v15 = if s.last_point.is_none() && s.claims.is_empty() {
            ExecutionCohortCensusV15::genesis_v15()
        } else {
            ExecutionCohortCensusV15::default()
        };
        s.execution_cohort_census_v15.compact_complete=false;
        return Ok(());
    }

    s.execution_cohort_census_v15 = build_census_v15(s, p)?;
    Ok(())
}
fn build_census_v15(
    s: &PalwChainStateV2,
    p: &PalwStateParamsV2,
) -> Result<ExecutionCohortCensusV15, PalwStateV2Error> {
    let mut occurrences = 0u64;
    all_candidates(s, p, |_, _, _| {
        occurrences = occurrences
            .checked_add(1)
            .ok_or(bad("cohort candidate count overflow"))?;
        Ok(())
    })?;
    let mut scratch_q = TirNativeRequestsV1::new();
    scratch_q
        .exact_v1::<BTreeNodeEnvelopeV1<Key, u64>>(occurrences)
        .map_err(PalwStateV2Error::NativeResource)?;
    let scratch_lease = reserve_native_requests_v1(scratch_q.bytes_v1())
        .map_err(PalwStateV2Error::NativeResource)?;
    let mut unique = BTreeMap::<Key, u64>::new();
    let mut q = TirNativeRequestsV1::new();
    all_candidates(s, p, |id, c, seen| {
        if matches!(seen,Seen::S1(..)){q.exact_v1::<BTreeNodeEnvelopeV1<(Hash64,Hash64),CompactRootSlotV15>>(1).map_err(PalwStateV2Error::NativeResource)?;}
        if c.execution_root == Hash64::default() {
            return Ok(());
        }
        let heap = c.native_heap_v1("cohort unique canonical row max");
        let bytes = heap
            .bytes()
            .ok_or(heap)
            .map_err(PalwStateV2Error::NativeResource)?;
        let old = unique.get(&Key::of(id, c)).copied();
        if old.is_none() {
            q.exact_v1::<BTreeNodeEnvelopeV1<Key, Member>>(1)
                .map_err(PalwStateV2Error::NativeResource)?;
        }
        if bytes > old.unwrap_or(0) {
            q.add_v1(bytes - old.unwrap_or(0))
                .map_err(PalwStateV2Error::NativeResource)?;
            unique.insert(Key::of(id, c), bytes);
        }
        if matches!(seen, Seen::Frame(_) | Seen::NativeBirth(_)) {
            q.exact_v1::<BTreeNodeEnvelopeV1<(Key, Clock), ()>>(1)
                .map_err(PalwStateV2Error::NativeResource)?;
        }
        Ok(())
    })?;
    drop(unique);
    drop(scratch_lease);
    let frame_count = rewards::frames(s).count() as u64;
    q.exact_v1::<BTreeNodeEnvelopeV1<Clock, ()>>(
        frame_count
            .checked_mul(2)
            .ok_or(bad("cohort bootstrap frame count overflow"))?,
    )
    .map_err(PalwStateV2Error::NativeResource)?;
    q.exact_v1::<CensusResidencyV15>(1)
        .map_err(PalwStateV2Error::NativeResource)?;
    q.exact_v1::<usize>(3)
        .map_err(PalwStateV2Error::NativeResource)?;
    let lease =
        reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    let mut census = ExecutionCohortCensusV15 {
        boundary: s.last_point,
        legacy_base_present: frame_count != 0 || s.last_point.is_none(),
        ..Default::default()
    };
    for (_, raw) in rewards::frames(s) {
        let f = rewards::decoded::<PalwClaimLiabilityFrameV1>(raw)?;
        census.frames.insert(Clock::of(f.point));
        census.bootstrap_frames.insert(Clock::of(f.point));
    }
    all_candidates(s, p, |id, c, seen| {
        add_candidate(&mut census, id, c, seen);
        Ok(())
    })?;
    census.ready = true;
    census.compact_complete = true;
    // Honest executed points are strictly blue-increasing and DAA-monotone.
    // An unsupported contradictory inventory is Missing, not an invented set.
    let mut previous = None;
    for at in &census.frames {
        if previous.is_some_and(|old: Clock| old.blue == at.blue || old.daa > at.daa) {
            census.ready = false;
        }
        previous = Some(*at);
    }
    census.residency = Some(std::sync::Arc::new(CensusResidencyV15 { _lease: lease }));
    census.tracking_active = true;
    Ok(census)
}
fn bounds(root: Hash64) -> (Key, Key) {
    let lo = Hash64::default();
    let hi = Hash64::from_bytes([255; 64]);
    (
        Key {
            root,
            id: lo,
            birth: Stored {
                block: lo,
                daa: 0,
                blue: 0,
            },
        },
        Key {
            root,
            id: hi,
            birth: Stored {
                block: hi,
                daa: u64::MAX,
                blue: u64::MAX,
            },
        },
    )
}
fn eligible(
    c: &ExecutionCohortCensusV15,
    key: Key,
    m: &Member,
    target: PalwBlockContextV2,
    current_live: bool,
) -> bool {
    if key.birth.daa > target.daa_score || key.birth.blue > target.blue_score {
        return false;
    }
    // S1 denial remains the old separate shared_root query. It does not turn
    // this member into a live/F-visible source or establish whole closure here.
    for (_, at) in c.observations.range(
        (
            key,
            Clock {
                blue: 0,
                daa: 0,
                block: Hash64::default(),
            },
        )
            ..=(
                key,
                Clock {
                    blue: u64::MAX,
                    daa: u64::MAX,
                    block: Hash64::from_bytes([255; 64]),
                },
            ),
    ) {
        if at.before(target) && c.frames.contains(at) {
            return true;
        }
    }
    let Some(interval) = m.interval else {
        return false;
    };
    interval_visible_v15(&c.frames, interval, target, m.current && current_live)
}
fn interval_visible_v15(
    frames: &BTreeSet<Clock>,
    interval: Interval,
    target: PalwBlockContextV2,
    current_live: bool,
) -> bool {
    // Full actual journal also supplies same in-flight block visibility before
    // the F writer emits its point, including transient insert/remove.
    if interval.death.is_some_and(|p| p == Clock::of(target)) {
        return true;
    }
    if current_live
        && interval.start.before(target)
        && (interval.inclusive || interval.start.blue < target.blue_score)
    {
        return true;
    }
    use std::ops::Bound::{Excluded, Included};
    let start = if interval.inclusive {
        Included(Clock {
            blue: interval.start.blue,
            daa: 0,
            block: Hash64::default(),
        })
    } else {
        Excluded(Clock {
            blue: interval.start.blue,
            daa: u64::MAX,
            block: Hash64::from_bytes([255; 64]),
        })
    };
    let upper = Clock {
        blue: target.blue_score,
        daa: u64::MAX,
        block: Hash64::from_bytes([255; 64]),
    };
    if interval.start.blue > target.blue_score {
        return false;
    }
    for at in frames.range((start, Included(upper))) {
        if !at.before(target) {
            break;
        }
        if interval
            .death
            .is_some_and(|death| at.blue > death.blue || at.daa > death.daa)
        {
            break;
        }
        return true;
    }
    false
}
/// Borrowed ONE authentic native row per source cohort; false means no complete
/// inventory and invokes the unchanged legacy reader. No callbacks on Missing.
pub(super) fn visit_birth_eligible_members_v15(
    s: &PalwChainStateV2,
    root: Hash64,
    consumer: PalwBlockContextV2,
    mut visit: impl FnMut(Hash64, &PalwClaimStateV2) -> Result<(), PalwStateV2Error>,
) -> Result<bool, PalwStateV2Error> {
    if root == Hash64::default() {
        return Ok(true);
    }
    let c = &s.execution_cohort_census_v15;
    if !c.ready {
        return Ok(false);
    }
    let mut owned_count = 0u64;
    let rooted_complete=super::palw_claim_local_execution_cohort_archive_v15::visit_current_root_cohort_members_v15(s,root,consumer,|_,_|{owned_count=owned_count.checked_add(1).ok_or(bad("cohort current member count overflow"))?;Ok(())})?;
    if !c.legacy_base_present && !rooted_complete && !native_root_members_complete_v22(s,root,consumer)? {
        return Ok(false);
    }
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<BTreeNodeEnvelopeV1<Key, ()>>(owned_count)
        .map_err(PalwStateV2Error::NativeResource)?;
    let _owned_keys =
        reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    let mut owned_keys = BTreeSet::new();
    if rooted_complete {
        super::palw_claim_local_execution_cohort_archive_v15::visit_current_root_cohort_members_v15(s,root,consumer,|id,row|{if !owned_keys.insert(Key::of(id,row)){return Err(bad("cohort duplicate exact stored root member"))}Ok(())})?;
    }
    let (lo, hi) = bounds(root);
    let qualifies = |key: Key, m: &Member| -> Result<bool, PalwStateV2Error> {
        if owned_keys.contains(&key) {
            return Ok(false);
        }
        if key.birth.daa > consumer.daa_score || key.birth.blue > consumer.blue_score {
            return Ok(false);
        }
        let current = s
            .claims
            .get(&key.id)
            .is_some_and(|row| Key::of(key.id, row) == key && same_identity(row, &m.row));
        if eligible(c, key, m, consumer, current) {
            return Ok(true);
        }
        if rooted_complete&&super::palw_claim_local_execution_cohort_archive_v15::current_cohort_contains_member_v15(s,root,key.id,&m.row)?{return Ok(true)}
        if let Some(point) = m.actual_birth {
            if point.daa_score <= consumer.daa_score && point.blue_score <= consumer.blue_score {
                if let Some(origin) =
                    super::palw_claim_local_source_admission_v8::for_accepting_point_v8(
                        s, key.id, point,
                    )?
                {
                    if !same_identity(origin.original_claim(), &m.row) {
                        return Err(bad("cohort scoped source immutable identity changed"));
                    }
                    return Ok(true);
                }
            }
        }
        if m.a5_body.is_some() {
            if let Some(closed) =
                super::palw_claim_local_exact_closed_source_v5::read_original_birth_v5(
                    s, key.id, &m.row,
                )?
            {
                if closed.birth.daa_score <= consumer.daa_score
                    && closed.birth.blue_score <= consumer.blue_score
                {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    };
    // Preflight availability before ANY callback, so Missing is not partial.
    let available = |key: Key, m: &Member| -> Result<bool, PalwStateV2Error> {
        if m.ambiguous {
            return Ok(false);
        }
        if rooted_complete&&super::palw_claim_local_execution_cohort_archive_v15::current_cohort_contains_member_v15(s,root,key.id,&m.row)?{return Ok(true)}
        if let Some(point) = m.actual_birth {
            if super::palw_claim_local_source_admission_v8::for_accepting_point_v8(
                s, key.id, point,
            )?
            .is_some()
            {
                return Ok(true);
            }
        }
        if super::palw_claim_local_exact_closed_source_v5::read_original_birth_v5(
            s, key.id, &m.row,
        )?
        .is_some()
        {
            return Ok(true);
        }
        if super::palw_claim_local_compact_sources_v1::read(s, key.id)?
            .as_ref()
            .is_some_and(|closed| same_identity(&closed.original, &m.row))
        {
            return Ok(true);
        }
        Ok(m.native_birth
            .is_some_and(|point| c.frames.contains(&point)))
    };
    for (key, m) in c.members.range(lo..=hi) {
        if qualifies(*key, m)? && !available(*key, m)? {
            return Ok(false);
        }
    }
    if rooted_complete {
        super::palw_claim_local_execution_cohort_archive_v15::visit_current_root_cohort_members_v15(s,root,consumer,|id,row|visit(id,row))?;
    }
    for (key, m) in c.members.range(lo..=hi) {
        if !qualifies(*key, m)? {
            continue;
        }
        if rooted_complete {
            if let Some(row) =
                super::palw_claim_local_execution_cohort_archive_v15::current_cohort_member_v15(
                    s, root, key.id, &m.row,
                )?
            {
                visit(key.id, &row)?;
                continue;
            }
        }
        if let Some(point) = m.actual_birth {
            if let Some(origin) =
                super::palw_claim_local_source_admission_v8::for_accepting_point_v8(
                    s, key.id, point,
                )?
            {
                visit(key.id, origin.original_claim())?;
                continue;
            }
        }
        if let Some(closed) =
            super::palw_claim_local_exact_closed_source_v5::read_original_birth_v5(
                s, key.id, &m.row,
            )?
        {
            visit(key.id, &closed.original)?;
            continue;
        }
        if let Some(closed) = super::palw_claim_local_compact_sources_v1::read(s, key.id)?
            .filter(|closed| same_identity(&closed.original, &m.row))
        {
            visit(key.id, &closed.original)?;
            continue;
        }
        // Only a still-rooted actual native birth justifies the cache body.
        visit(key.id, &m.row)?;
    }
    Ok(true)
}
#[derive(Clone)]
enum Change {
    Born(Key, PalwClaimStateV2, bool),
    Died(Key),
}
/// Called ONCE for the completed actual native journal before owner15 snapshot
/// capture. All new same-block claims are seen before any cohort is pinned.
pub(super) fn observe_fold_v15(
    b: &mut TransitionBuilder<'_>,
    _parent: &PalwChainStateV2,
    ctx: &PalwBlockContextV2,
) -> Result<(), PalwStateV2Error> {
    if !b.params.claim_local_settlement_active_at(ctx.daa_score) {
        // No prefence index allocation. A changed native source means this
        // cache cannot later pretend its genesis empty baseline stayed complete.
        if b.entries.iter().any(|e|matches!(e,PalwDeltaEntryV2::Claim{key,old,new}if old.as_ref().map(|c|Key::of(*key,c))!=new.as_ref().map(|c|Key::of(*key,c)))){b.state.execution_cohort_census_v15.ready=false;}
        return Ok(());
    }

    b.state.execution_cohort_census_v15.tracking_active = true;
    for e in &b.entries {
        if let PalwDeltaEntryV2::Claim {
            key,
            old: Some(old),
            new: Some(new),
        } = e
        {
            if Key::of(*key, old) == Key::of(*key, new) && !same_identity(old, new) {
                if let Some(member) = b
                    .state
                    .execution_cohort_census_v15
                    .members
                    .get_mut(&Key::of(*key, old))
                {
                    member.ambiguous = true;
                }
            }
        }
    }

    let mut count = 0u64;
    let mut q = TirNativeRequestsV1::new();
    for e in &b.entries {
        if let PalwDeltaEntryV2::Claim { old, new, .. } = e {
            if let Some(c) = new {
                if old
                    .as_ref()
                    .is_none_or(|o| Key::of(Hash64::default(), o) != Key::of(Hash64::default(), c))
                {
                    count = count
                        .checked_add(1)
                        .ok_or(bad("cohort journal births overflow"))?;
                    q.exact_v1::<BTreeNodeEnvelopeV1<Key, Member>>(1)
                        .map_err(PalwStateV2Error::NativeResource)?;
                    let heap = c.native_heap_v1("cohort actual birth row");
                    q.add_v1(
                        heap.bytes()
                            .ok_or(heap)
                            .map_err(PalwStateV2Error::NativeResource)?,
                    )
                    .map_err(PalwStateV2Error::NativeResource)?;
                }
            }
            if old.as_ref().is_some_and(|o| {
                new.as_ref()
                    .is_none_or(|n| Key::of(Hash64::default(), o) != Key::of(Hash64::default(), n))
            }) {
                count = count
                    .checked_add(1)
                    .ok_or(bad("cohort journal deaths overflow"))?;
            }
        }
    }
    q.exact_v1::<Change>(count)
        .map_err(PalwStateV2Error::NativeResource)?;
    b.native_reservation
        .grow_v1(q.bytes_v1())
        .map_err(PalwStateV2Error::NativeResource)?;
    let mut changes = Vec::with_capacity(
        usize::try_from(count).map_err(|_| bad("cohort change count exceeds address space"))?,
    );
    for e in &b.entries {
        if let PalwDeltaEntryV2::Claim { key, old, new } = e {
            let before = old.as_ref().map(|c| Key::of(*key, c));
            let after = new.as_ref().map(|c| Key::of(*key, c));
            if before != after {
                if let Some(k) = before {
                    changes.push(Change::Died(k));
                }
                if let Some(c) = new {
                    changes.push(Change::Born(
                        Key::of(*key, c),
                        c.clone(),
                        old.is_none()
                            && c.accepted_daa == ctx.daa_score
                            && c.accepted_blue_score == ctx.blue_score,
                    ));
                }
            }
        }
    }
    let census = &mut b.state.execution_cohort_census_v15;
    for change in changes {
        match change {
            Change::Died(k) => {
                if let Some(m) = census.members.get_mut(&k) {
                    m.current = false;
                    if let Some(i) = &mut m.interval {
                        i.death = Some(Clock::of(*ctx));
                    }
                } else {
                    census.ready = false
                }
            }
            Change::Born(k, row, actual) => {
                let m = census.members.entry(k).or_insert_with(|| Member {
                    row,
                    actual_birth: None,
                    scoped_birth: false,
                    current: false,
                    interval: None,
                    ambiguous: false,
                    s1_body: None,
                    a5_body: None,
                    owned: false,
                    canonical_rank: 9,
                    native_birth: None,
                });
                if m.current || m.interval.is_some_and(|i| i.death.is_some()) {
                    m.ambiguous = true;
                }
                if actual {
                    m.canonical_rank = m.canonical_rank.min(2);
                    m.native_birth = Some(Clock::of(*ctx));
                    if m.actual_birth.is_some_and(|p| p != *ctx) {
                        m.ambiguous = true
                    } else {
                        m.actual_birth = Some(*ctx);
                        m.scoped_birth = true
                    }
                } else {
                    m.ambiguous = true
                }
                m.current = true;
                m.interval = Some(Interval {
                    start: Clock::of(*ctx),
                    inclusive: true,
                    death: None,
                });
            }
        }
    }
    Ok(())
}
pub(super) fn frame_change_requests_v15(
    key: &[u8],
    new: Option<&[u8]>,
) -> Result<u64, NativeCoverageV1> {
    if key.len() != 65 || key[0] != b'F' {
        return Ok(0);
    }
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<u8>(65)?;
    if new.is_some() {
        q.exact_v1::<BTreeNodeEnvelopeV1<Clock, ()>>(1)?;
    }
    Ok(q.bytes_v1())
}
/// Held by the returned replay state owner, not only a temporary delta scope.
pub(super) fn replay_requests_v15(
    state: &PalwChainStateV2,
    delta: &PalwStateDeltaV2,
) -> Result<u64, NativeCoverageV1> {
    if !state.execution_cohort_census_v15.tracking_active&&!delta.entries.iter().any(|e|matches!(e,PalwDeltaEntryV2::KernelRouteRow{table,key,..}if *table==PALW_CLAIM_LOCAL_TABLE_V1&&key.len()==65&&matches!(key[0],b'F'|b'S'))){return Ok(0)}
    let mut q = TirNativeRequestsV1::new();
    for e in &delta.entries {
        match e {
            PalwDeltaEntryV2::Claim { old, new, .. } => {
                if old.as_ref().zip(new.as_ref()).is_some_and(|(a, b)| {
                    Key::of(Hash64::default(), a) == Key::of(Hash64::default(), b)
                }) {
                    continue;
                }
                for row in [old.as_ref(), new.as_ref()].into_iter().flatten() {
                    q.exact_v1::<BTreeNodeEnvelopeV1<Key, Member>>(1)?;
                    let heap = row.native_heap_v1("cohort replay canonical row");
                    q.add_v1(heap.bytes().ok_or(heap)?)?;
                }
            }
            PalwDeltaEntryV2::KernelRouteRow {
                table,
                key,
                old,
                new,
            } if *table == PALW_CLAIM_LOCAL_TABLE_V1 => {
                q.add_v1(frame_change_requests_v15(key, new.as_deref())?)?;
                q.add_v1(frame_change_requests_v15(key, old.as_deref())?)?;
                if compact_key_v15(*table,key){q.add_v1(compact_growth_v15(old.is_some())?)?;q.add_v1(compact_growth_v15(new.is_some())?)?;}
            }
            _ => {}
        }
    }
    Ok(q.bytes_v1())
}
/// Complete delta after primary replay; only exact original native birth
/// creates/removes a member. Inverse death restores its previous live interval.
pub(super) fn replay_fold_v15(
    s: &mut PalwChainStateV2,
    delta: &PalwStateDeltaV2,
    revert: bool,
    params: &PalwStateParamsV2,
) -> Result<(), PalwStateV2Error> {
    if !params.claim_local_settlement_active_at(delta.point.daa_score) {
        if delta.entries.iter().any(|e|matches!(e,PalwDeltaEntryV2::Claim{key,old,new}if old.as_ref().map(|c|Key::of(*key,c))!=new.as_ref().map(|c|Key::of(*key,c)))){s.execution_cohort_census_v15.ready=false;}
        return Ok(());
    }
    s.execution_cohort_census_v15.tracking_active = true;
    let restore_boundary = s.last_point.map(Clock::of);
    let update = |census: &mut ExecutionCohortCensusV15, e: &PalwDeltaEntryV2| {
        let PalwDeltaEntryV2::Claim { key, old, new } = e else {
            return;
        };
        let before = old.as_ref().map(|c| Key::of(*key, c));
        let after = new.as_ref().map(|c| Key::of(*key, c));
        if before == after {
            return;
        }
        let point = Clock::of(delta.point);
        if revert {
            if let Some(k) = after {
                if old.is_none()
                    && new.as_ref().is_some_and(|row| {
                        row.accepted_daa == delta.point.daa_score
                            && row.accepted_blue_score == delta.point.blue_score
                    })
                {
                    census.members.remove(&k);
                } else {
                    census.ready = false;
                }
            }
            if let Some(k) = before {
                if let Some(m) = census.members.get_mut(&k) {
                    m.current = true;
                    if let Some(i) = &mut m.interval {
                        if i.death == Some(point) {
                            i.death = None
                        }
                    }
                } else if let Some(row) = old {
                    // Restored actual old native row is an observation only.
                    // Its original comes from restored Root15/A8/A5/S1 before
                    // any callback; unknown phase never becomes an original.
                    census.members.insert(
                        k,
                        Member {
                            row: row.clone(),
                            actual_birth: None,
                            scoped_birth: false,
                            current: true,
                            interval: Some(Interval {
                                start: restore_boundary.unwrap_or(point),
                                inclusive: false,
                                death: None,
                            }),
                            ambiguous: false,
                            s1_body: None,
                            a5_body: None,
                            owned: false,
                            canonical_rank: 9,
                            native_birth: None,
                        },
                    );
                }
            }
        } else {
            if let Some(k) = before {
                if let Some(m) = census.members.get_mut(&k) {
                    m.current = false;
                    if let Some(i) = &mut m.interval {
                        i.death = Some(point)
                    }
                } else {
                    census.ready = false
                }
            }
            if let Some(row) = new {
                let k = Key::of(*key, row);
                let actual = old.is_none()
                    && row.accepted_daa == delta.point.daa_score
                    && row.accepted_blue_score == delta.point.blue_score;
                if census.members.contains_key(&k) {
                    census.ready = false;
                    return;
                }
                census.members.insert(
                    k,
                    Member {
                        row: row.clone(),
                        actual_birth: actual.then_some(delta.point),
                        scoped_birth: actual,
                        current: true,
                        interval: Some(Interval {
                            start: point,
                            inclusive: true,
                            death: None,
                        }),
                        ambiguous: !actual,
                        s1_body: None,
                        a5_body: None,
                        owned: false,
                        canonical_rank: if actual { 2 } else { 9 },
                        native_birth: actual.then_some(point),
                    },
                );
            }
        }
    };
    if revert {
        for e in delta.entries.iter().rev() {
            update(&mut s.execution_cohort_census_v15, e)
        }
    } else {
        for e in &delta.entries {
            update(&mut s.execution_cohort_census_v15, e)
        }
    }
    Ok(())
}
#[derive(Clone, Copy)]
pub(super) struct FrameChangeV15 {
    before: Option<Clock>,
    after: Option<Clock>,
    inconsistent: bool,
}
/// Pure actual current native lookup before map mutation. All fallible codec
/// and resource work precedes the original writer. Commit is infallible.
pub(super) fn prepare_frame_change_v15(
    s: &PalwChainStateV2,
    table: u8,
    key: &[u8],
    new: Option<&[u8]>,
) -> Result<Option<FrameChangeV15>, PalwStateV2Error> {
    use borsh::BorshDeserialize;
    if table != PALW_CLAIM_LOCAL_TABLE_V1 || key.len() != 65 || key[0] != b'F' {
        return Ok(None);
    }
    let _key = reserve_native_requests_v1(65).map_err(PalwStateV2Error::NativeResource)?;
    let old = s
        .kernel_route()
        .and_then(|r| r.aux.get(&(table, key.to_vec())));
    let parse = |raw: &[u8]| {
        let mut reader = raw;
        let p = PalwBlockContextV2::deserialize_reader(&mut reader)
            .map_err(|_| bad("cohort F point codec"))?;
        if &key[1..] != p.block.as_byte_slice() {
            return Err(bad("cohort F key differs from actual point"));
        }
        Ok(Clock::of(p))
    };
    let before = old.map(|raw| parse(raw)).transpose()?;
    let after = new.map(parse).transpose()?;
    let inconsistent = s.execution_cohort_census_v15.ready
        && before.is_some_and(|p| !s.execution_cohort_census_v15.frames.contains(&p));
    Ok(Some(FrameChangeV15 {
        before,
        after,
        inconsistent,
    }))
}
pub(super) fn commit_frame_change_v15(s: &mut PalwChainStateV2, p: FrameChangeV15) {
    let c = &mut s.execution_cohort_census_v15;
    if p.inconsistent {
        c.ready = false
    }
    if p.before == p.after {
        return;
    }
    if let Some(before) = p.before {
        c.frames.remove(&before);
    }
    if let Some(after) = p.after {
        if c.boundary.is_some_and(|at| after.blue <= at.blue_score)
            && !c.bootstrap_frames.contains(&after)
        {
            c.needs_global_rebuild = true;
        }
        c.frames.insert(after);
        if c.ready {
            c.legacy_base_present = true;
        }
    }
}

pub(super) fn prepare_writer_v15(
    b: &mut TransitionBuilder<'_>,
    table: u8,
    key: &[u8],
    new: Option<&[u8]>,
) -> Result<Option<FrameChangeV15>, PalwStateV2Error> {
    if table != PALW_CLAIM_LOCAL_TABLE_V1 {
        return Ok(None);
    }
    let requests = frame_change_requests_v15(key, new).map_err(PalwStateV2Error::NativeResource)?;
    b.native_reservation
        .grow_v1(requests)
        .map_err(PalwStateV2Error::NativeResource)?;
    prepare_frame_change_v15(&b.state, table, key, new)
}

#[cfg(test)]
mod interval_tests {
    use super::*;
    fn p(n: u64) -> PalwBlockContextV2 {
        PalwBlockContextV2 {
            block: Hash64::from_u64_word(n),
            daa_score: n,
            blue_score: n,
            subsidy: 0,
        }
    }
    #[test]
    fn rfc23s2_cohort_interval_cold_boundary_death_and_frame_collection() {
        // Geometry only; no fabricated Claim/source/approval is admitted here.
        let mut frames = BTreeSet::from([Clock::of(p(10))]);
        let cold = Interval {
            start: Clock::of(p(10)),
            inclusive: false,
            death: Some(Clock::of(p(12))),
        };
        assert!(
            !interval_visible_v15(&frames, cold, p(10), false),
            "restorepoint alone is not an older F observation"
        );
        frames.insert(Clock::of(p(11)));
        assert!(interval_visible_v15(&frames, cold, p(13), false));
        frames.remove(&Clock::of(p(11)));
        frames.insert(Clock::of(p(13)));
        assert!(
            !interval_visible_v15(&frames, cold, p(13), false),
            "later unrelated frame cannot cross physical death"
        );
        assert!(
            interval_visible_v15(&frames, cold, p(12), false),
            "actual death journal is inclusive before F publication"
        );
        let born = Interval {
            start: Clock::of(p(20)),
            inclusive: true,
            death: Some(Clock::of(p(20))),
        };
        assert!(
            interval_visible_v15(&BTreeSet::new(), born, p(20), false),
            "same-block transient birth/removal remains visible"
        );
        assert!(
            !interval_visible_v15(&BTreeSet::new(), born, p(21), false),
            "no retained F or typed authority is not permanent membership"
        );
    }
}

/// Opaque global activation preparation. No producer/source argument, paid
/// permit or partial world is accepted. The genuine complete parent root and
/// actual next context bind this single-use native boundary operation.
pub(super) struct PreparedCohortMigrationV15 {
    parent_root: Hash64,
    parent_point: Option<PalwBlockContextV2>,
    executing: PalwBlockContextV2,
    census: ExecutionCohortCensusV15,
    canonical_bytes: u64,
    _root_workspace: Option<crate::palw_native_resource_v1::NativeDemandLeaseV1>,
}
impl PreparedCohortMigrationV15 {
    pub(super) fn parent_root(&self) -> Hash64 {
        self.parent_root
    }
    pub(super) fn canonical_bytes(&self) -> u64 {
        self.canonical_bytes
    }
    pub(super) fn retained_requests(&self) -> Result<u64, NativeCoverageV1> {
        let heap = self
            .census
            .native_heap_v1("prepared native cohort migration");
        heap.bytes().ok_or(heap)
    }
}
/// Explicit first-active coordinator ONLY, before objects. Full-world temporary
/// carriage/root scratch and complete census have their own native leases;
/// ordinary source quotes never pay this global migration operation.
pub(super) fn prepare_global_migration_v15(
    parent: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    ctx: &PalwBlockContextV2,
) -> Result<PreparedCohortMigrationV15, PalwStateV2Error> {
    if !params.claim_local_settlement_active_at(ctx.daa_score)
        || parent
            .last_point
            .is_some_and(|p| params.claim_local_settlement_active_at(p.daa_score))
    {
        return Err(bad(
            "cohort global migration is not actual first activation boundary",
        ));
    }
    if parent
        .last_point
        .is_some_and(|p| p.blue_score >= ctx.blue_score || p.daa_score > ctx.daa_score)
    {
        return Err(bad(
            "cohort global migration context is not next actual native point",
        ));
    }
    let requests = crate::palw_native_resource_v1::native_value_requests_v1(parent)
        .map_err(PalwStateV2Error::NativeResource)?;
    let clone_lease =
        reserve_native_requests_v1(requests).map_err(PalwStateV2Error::NativeResource)?;
    // A genuine complete carriage at an explicitly global boundary, never a
    // projected partial State or permanent shadow for a source consumer.
    let carriage = PalwStateCarriageV2::from_state(parent);
    let canonical_bytes = borsh::object_length(&carriage)
        .map_err(|_| bad("global cohort parent carriage length"))? as u64;
    let root_workspace =
        crate::palw_native_resource_v1::reserve_native_root_workspace_v1(canonical_bytes)
            .map_err(PalwStateV2Error::NativeResource)?;
    let parent_root = parent.state_root();
    drop(carriage);
    drop(clone_lease);
    let census = build_census_v15(parent, params)?;
    Ok(PreparedCohortMigrationV15 {
        parent_root,
        parent_point: parent.last_point,
        executing: *ctx,
        census,
        canonical_bytes,
        _root_workspace: root_workspace,
    })
}
pub(super) fn install_global_migration_v15(
    b: &mut TransitionBuilder<'_>,
    prepared: PreparedCohortMigrationV15,
) -> Result<(), PalwStateV2Error> {
    if !b.entries.is_empty()
        || b.claim_local_lock_point_v6 != Some(prepared.executing)
        || b.state.last_point != prepared.parent_point
        || b.state.state_root() != prepared.parent_root
    {
        return Err(bad(
            "cohort global migration does not bind unchanged actual native parent",
        ));
    }
    b.state.execution_cohort_census_v15 = prepared.census;
    Ok(())
}
/// Callable actual first-active path, once per activation, never a per-source
/// lazy rebuild. Unrecoverable old canonical origins still return real Missing.
pub(super) fn prepare_first_active_boundary_v15(
    b: &mut TransitionBuilder<'_>,
    parent: &PalwChainStateV2,
    ctx: &PalwBlockContextV2,
) -> Result<(), PalwStateV2Error> {
    if b.params.claim_local_settlement_active_at(ctx.daa_score)
        && (!parent.execution_cohort_census_v15.ready || !parent.execution_cohort_census_v15.compact_complete)
        && parent
            .last_point
            .is_none_or(|p| !b.params.claim_local_settlement_active_at(p.daa_score))
    {
        let prepared = prepare_global_migration_v15(parent, b.params, ctx)?;
        install_global_migration_v15(b, prepared)?;
    }
    Ok(())
}

/// Partial object/preparation rehearsal is NOT an accepted block birth batch.
/// It may never leave a changed native source behind a falsely complete cache.
/// No allocation, synthetic observation or new authority is created here.
pub(super) fn invalidate_partial_view_v15(b: &mut TransitionBuilder<'_>) {
    finish_compact_observations_v15(b);
    if b.entries.iter().any(|e|matches!(e,PalwDeltaEntryV2::Claim{key,old,new}if old.as_ref().map(|c|Key::of(*key,c))!=new.as_ref().map(|c|Key::of(*key,c)))){b.state.execution_cohort_census_v15.ready=false;}
}

/// Reorg/global native boundary ONLY, after every primary delta entry and
/// ordinary derived-index repair is complete. No partial replay view is named
/// as a complete state root; one missing old-F census is rebuilt transactionally.
pub(super) fn settle_replay_boundary_v15(
    state: &mut PalwChainStateV2,
    params: &PalwStateParamsV2,
) -> Result<(), PalwStateV2Error> {
    if !state.execution_cohort_census_v15.needs_global_rebuild {
        return Ok(());
    }
    if !state
        .last_point
        .is_some_and(|point| params.claim_local_settlement_active_at(point.daa_score))
    {
        state.execution_cohort_census_v15 = if state.last_point.is_none() && state.claims.is_empty()
        {
            ExecutionCohortCensusV15::genesis_v15()
        } else {
            ExecutionCohortCensusV15::default()
        };
        state.execution_cohort_census_v15.compact_complete=false;
        return Ok(());
    }
    let native_requests = crate::palw_native_resource_v1::native_value_requests_v1(state)
        .map_err(PalwStateV2Error::NativeResource)?;
    let clone_lease =
        reserve_native_requests_v1(native_requests).map_err(PalwStateV2Error::NativeResource)?;
    let actual_carriage = PalwStateCarriageV2::from_state(state);
    let bytes = borsh::object_length(&actual_carriage)
        .map_err(|_| bad("complete replay census carriage length"))? as u64;
    let root_lease = crate::palw_native_resource_v1::reserve_native_root_workspace_v1(bytes)
        .map_err(PalwStateV2Error::NativeResource)?;
    let actual_complete_basis = state.state_root();
    let actual_point = state.last_point;
    drop(actual_carriage);
    drop(clone_lease);
    let prepared = build_census_v15(state, params)?;
    // No await, callback or primary mutation occurs between the authenticated
    // full-view observation and this derived-field-only install. Unique &mut
    // ownership keeps the same actual complete basis; no second world hash.
    let _binding = (actual_complete_basis, actual_point);
    state.execution_cohort_census_v15 = prepared;
    drop(root_lease);
    Ok(())
}

#[derive(Clone, Copy)]
enum KnownAuthorityV15 {
    Root,
    A8(PalwBlockContextV2),
    A5(PalwBlockContextV2),
    S1(PalwBlockContextV2),
    CachedBirth,
}
#[derive(Clone, Copy)]
struct KnownReadPlanV15 {
    key: Key,
    authority: KnownAuthorityV15,
}
/// Read-only semantic join. Coverage is computed from native metadata BEFORE
/// callbacks; it is never a paid/full-snapshot permit. A callback Break stops
/// immediately, with no later Claim body hydration or status error overriding
/// a genuine known Denied. Real callback errors preserve their native class.
pub(super) fn visit_known_birth_members_v15(
    s: &PalwChainStateV2,
    root: Hash64,
    consumer: PalwBlockContextV2,
    mut visit: impl FnMut(
        Hash64,
        &PalwClaimStateV2,
    ) -> Result<std::ops::ControlFlow<()>, PalwStateV2Error>,
) -> Result<bool, PalwStateV2Error> {
    use super::palw_claim_local_execution_cohort_archive_v15 as owned;
    use std::ops::ControlFlow;
    if root == Hash64::default() {
        return Ok(true);
    }
    let c = &s.execution_cohort_census_v15;
    let root_count = owned::current_cohort_member_count_at_v15(s, root, consumer)?;
    let mut complete = c.ready && (c.legacy_base_present || root_count.is_some() || native_root_members_complete_v22(s,root,consumer)?);
    let (lo, hi) = bounds(root);
    let count = c.members.range(lo..=hi).count() as u64;
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<KnownReadPlanV15>(count)
        .map_err(PalwStateV2Error::NativeResource)?;
    q.exact_v1::<BTreeNodeEnvelopeV1<Key, ()>>(root_count.unwrap_or(0))
        .map_err(PalwStateV2Error::NativeResource)?;
    let _plans =
        reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    let mut plans = Vec::with_capacity(
        usize::try_from(count).map_err(|_| bad("cohort known plan count exceeds address space"))?,
    );
    for (key, m) in c.members.range(lo..=hi) {
        if key.birth.daa > consumer.daa_score || key.birth.blue > consumer.blue_score {
            continue;
        }
        if owned::current_cohort_member_reference_at_v15(s, root, key.id, &m.row, consumer)? {
            plans.push(KnownReadPlanV15 {
                key: *key,
                authority: KnownAuthorityV15::Root,
            });
            continue;
        }
        let current = s
            .claims
            .get(&key.id)
            .is_some_and(|row| Key::of(key.id, row) == *key && same_identity(row, &m.row));
        let geometric = c.ready && eligible(c, *key, m, consumer, current);
        let point = m
            .actual_birth
            .filter(|p| p.daa_score <= consumer.daa_score && p.blue_score <= consumer.blue_score);
        let a8 = point.is_some()
            && super::palw_claim_local_source_admission_v8::has_source_admission_v8(
                s, key.id, &m.row,
            )?;
        let a5 = point.is_some()
            && super::palw_claim_local_exact_closed_source_v5::original_birth_present_v15(
                s, key.id, &m.row,
            )?;
        let s1 = if point.is_some() {
            if let Some(hash) = m.s1_body {
                let _key =
                    reserve_native_requests_v1(65).map_err(PalwStateV2Error::NativeResource)?;
                let mut native_key = Vec::with_capacity(65);
                native_key.push(b'S');
                native_key.extend_from_slice(key.id.as_byte_slice());
                s.kernel_route()
                    .and_then(|r| r.aux.get(&(PALW_CLAIM_LOCAL_TABLE_V1, native_key)))
                    .is_some_and(|raw| {
                        crate::palw_claim_local_v1::palw_liability_material_id_v1(raw) == hash
                    })
            } else {
                false
            }
        } else {
            false
        };
        if !geometric && !a8 && !a5 && !s1 {
            continue;
        }
        if m.ambiguous || m.canonical_rank == 9 {
            complete = false;
            continue;
        }
        let authority = if a5 {
            Some(KnownAuthorityV15::A5(point.expect("checked actual point")))
        } else if a8 {
            Some(KnownAuthorityV15::A8(point.expect("checked actual point")))
        } else if s1 {
            Some(KnownAuthorityV15::S1(point.expect("checked actual point")))
        } else if c.ready && m.native_birth.is_some_and(|p| c.frames.contains(&p)) {
            Some(KnownAuthorityV15::CachedBirth)
        } else {
            None
        };
        if let Some(authority) = authority {
            plans.push(KnownReadPlanV15 {
                key: *key,
                authority,
            })
        } else {
            complete = false;
        }
    }
    // No source body after this point is touched unless the actual stream
    // reaches it. Root callback Break propagates through its native visitor.
    let mut emitted = BTreeSet::new();
    let mut stopped = false;
    if root_count.is_some() {
        owned::visit_current_root_cohort_members_until_v15(s, root, consumer, |id, row| {
            if !emitted.insert(Key::of(id, row)) {
                return Err(bad("known cohort duplicated stored root member"));
            }
            let flow = visit(id, row)?;
            if matches!(flow, ControlFlow::Break(())) {
                stopped = true;
            }
            Ok(flow)
        })?;
        if stopped {
            return Ok(complete);
        }
    }
    for plan in plans {
        if emitted.contains(&plan.key) {
            continue;
        }
        let m = c
            .members
            .get(&plan.key)
            .ok_or(bad("known cohort read plan lost immutable native member"))?;
        let flow = match plan.authority {
            KnownAuthorityV15::Root => {
                let row = owned::current_cohort_member_v15(s, root, plan.key.id, &m.row)?
                    .ok_or(bad("known rooted cohort member vanished"))?;
                visit(plan.key.id, &row)?
            }
            KnownAuthorityV15::A8(point) => {
                let row = super::palw_claim_local_source_admission_v8::for_accepting_point_v8(
                    s,
                    plan.key.id,
                    point,
                )?
                .ok_or(bad("known A8 cohort member vanished"))?;
                if !same_identity(row.original_claim(), &m.row) {
                    return Err(bad("known A8 cohort body identity differs"));
                }
                visit(plan.key.id, row.original_claim())?
            }
            KnownAuthorityV15::A5(point) => {
                let row = super::palw_claim_local_exact_closed_source_v5::read_original_birth_v5(
                    s,
                    plan.key.id,
                    &m.row,
                )?
                .ok_or(bad("known A5 cohort member vanished"))?;
                if row.birth != point || !same_identity(&row.original, &m.row) {
                    return Err(bad("known A5 cohort actual birth/body differs"));
                }
                visit(plan.key.id, &row.original)?
            }
            KnownAuthorityV15::S1(point) => {
                let row = super::palw_claim_local_compact_sources_v1::read(s, plan.key.id)?
                    .ok_or(bad("known S1 cohort member vanished"))?;
                if row.birth != point || !same_identity(&row.original, &m.row) {
                    return Err(bad("known S1 cohort actual birth/body differs"));
                }
                visit(plan.key.id, &row.original)?
            }
            KnownAuthorityV15::CachedBirth => visit(plan.key.id, &m.row)?,
        };
        if matches!(flow, ControlFlow::Break(())) {
            return Ok(complete);
        }
    }
    Ok(complete)
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CompactRootPointerV15 {
    point: PalwBlockContextV2,
    body: Hash64,
}
type CompactRootSlotV15 = Option<CompactRootPointerV15>;
#[derive(Clone, Copy, Debug)]
struct CompactSlotUndoV15 {
    key: (Hash64, Hash64),
    existed: bool,
    before: CompactRootSlotV15,
}
/// Transient actual native-journal observation, never serialized/status authority.
#[derive(Clone, Copy, Debug)]
pub(super) struct CompactRootUndoV15 {
    ordinal: usize,
    id: Hash64,
    slots: [Option<CompactSlotUndoV15>; 2],
    complete: bool,
    old_native: Option<Hash64>,
    new_native: Option<Hash64>,
}
#[derive(Clone, Copy)]
pub(super) struct CompactRootChangeV15 {
    before: Option<((Hash64, Hash64), CompactRootPointerV15)>,
    after: Option<((Hash64, Hash64), CompactRootPointerV15)>,
    invalidate: bool,
    absent: bool,
    ordinal: usize,
    undo: Option<CompactRootUndoV15>,
}
fn compact_key_v15(table: u8, key: &[u8]) -> bool {
    table == PALW_CLAIM_LOCAL_TABLE_V1 && key.len() == 65 && key[0] == b'S'
}
fn compact_pointer_v15(
    id: Hash64,
    raw: Option<&[u8]>,
) -> Result<Option<((Hash64, Hash64), CompactRootPointerV15)>, PalwStateV2Error> {
    raw.map(|raw| {
        let row = super::palw_claim_local_compact_sources_v1::decode_checked_index_v15(raw, id)?;
        Ok((
            (row.original.execution_root, id),
            CompactRootPointerV15 {
                point: row.birth,
                body: crate::palw_claim_local_v1::palw_liability_material_id_v1(raw),
            },
        ))
    })
    .transpose()
}
fn plan_compact_change_v15(
    s: &PalwChainStateV2,
    table: u8,
    key: &[u8],
    new: Option<&[u8]>,
) -> Result<Option<CompactRootChangeV15>, PalwStateV2Error> {
    if !compact_key_v15(table, key) || s.kernel_route.is_none() {
        return Ok(None);
    }
    let cache = &s.execution_cohort_census_v15;
    if !cache.compact_complete {
        return Ok(Some(CompactRootChangeV15 {
            before: None,
            after: None,
            invalidate: true,
            absent: false,
            ordinal: 0,
            undo: None,
        }));
    }
    let id = Hash64::from_bytes(
        key[1..]
            .try_into()
            .map_err(|_| bad("compact index source key"))?,
    );
    let _key = reserve_native_requests_v1(65).map_err(PalwStateV2Error::NativeResource)?;
    let old = s
        .kernel_route
        .as_ref()
        .and_then(|r| r.aux.get(&(table, key.to_vec())));
    let before = compact_pointer_v15(id, old.map(Vec::as_slice))?;
    let after = compact_pointer_v15(id, new)?;
    if let Some((key, pointer)) = before {
        if cache.compact_by_root.get(&key).copied().flatten() != Some(pointer) {
            return Err(bad("compact old native pointer differs"));
        }
    }
    if let Some((key, _)) = after {
        if before.map(|(k, _)| k) != Some(key)
            && cache.compact_by_root.get(&key).is_some_and(Option::is_some)
        {
            return Err(bad("compact new native pointer already exists"));
        }
    }
    let absent = after.is_some_and(|(key, _)| !cache.compact_by_root.contains_key(&key));
    Ok(Some(CompactRootChangeV15 {
        before,
        after,
        invalidate: false,
        absent,
        ordinal: 0,
        undo: None,
    }))
}
fn compact_growth_v15(absent: bool) -> Result<u64, NativeCoverageV1> {
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<BTreeNodeEnvelopeV1<(Hash64, Hash64), CompactRootSlotV15>>(u64::from(absent))?;
    Ok(q.bytes_v1())
}
fn reserve_compact_undo_v15(b: &mut TransitionBuilder<'_>) -> Result<(), PalwStateV2Error> {
    let needed = b
        .compact_root_undo_v15
        .len()
        .checked_add(1)
        .ok_or(bad("compact undo count overflow"))?;
    if needed <= b.compact_root_undo_v15.capacity() {
        return Ok(());
    }
    let mut cap = b.compact_root_undo_v15.capacity().max(2);
    while cap < needed {
        cap = cap
            .checked_mul(2)
            .ok_or(bad("compact undo capacity overflow"))?;
    }
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<CompactRootUndoV15>(cap as u64)
        .map_err(PalwStateV2Error::NativeResource)?;
    b.native_reservation
        .grow_v1(q.bytes_v1())
        .map_err(PalwStateV2Error::NativeResource)?;
    b.compact_root_undo_v15
        .try_reserve_exact(cap - b.compact_root_undo_v15.len())
        .map_err(|_| {
            PalwStateV2Error::NativeResource(NativeCoverageV1::unsupported::<CompactRootUndoV15>(
                "compact exact native undo",
                "allocator refused pre-admitted vector",
            ))
        })?;
    Ok(())
}
pub(super) fn prepare_compact_writer_v15(
    b: &mut TransitionBuilder<'_>,
    table: u8,
    key: &[u8],
    new: Option<&[u8]>,
) -> Result<Option<CompactRootChangeV15>, PalwStateV2Error> {
    if !compact_key_v15(table, key) {
        return Ok(None);
    }
    let ordinal = b.entries.len();
    let active = b
        .claim_local_lock_point_v6
        .or(b.state.last_point)
        .is_some_and(|p| b.params.claim_local_settlement_active_at(p.daa_score));
    if !active {
        return Ok(Some(CompactRootChangeV15 {
            before: None,
            after: None,
            invalidate: true,
            absent: false,
            ordinal,
            undo: None,
        }));
    }
    let Some(mut p) = plan_compact_change_v15(&b.state, table, key, new)? else {
        return Ok(None);
    };
    p.ordinal = ordinal;
    b.native_reservation
        .grow_v1(compact_growth_v15(p.absent).map_err(PalwStateV2Error::NativeResource)?)
        .map_err(PalwStateV2Error::NativeResource)?;
    if !p.invalidate && p.before != p.after {
        let map = &b.state.execution_cohort_census_v15.compact_by_root;
        let mut slots = [None, None];
        if let Some((key, _)) = p.before {
            slots[0] = Some(CompactSlotUndoV15 {
                key,
                existed: map.contains_key(&key),
                before: map.get(&key).copied().flatten(),
            });
        }
        if let Some((key, _)) = p.after {
            if p.before.map(|(key, _)| key) != Some(key) {
                slots[1] = Some(CompactSlotUndoV15 {
                    key,
                    existed: map.contains_key(&key),
                    before: map.get(&key).copied().flatten(),
                });
            }
        }
        p.undo = Some(CompactRootUndoV15 {
            ordinal,
            id: Hash64::from_bytes(
                key[1..]
                    .try_into()
                    .map_err(|_| bad("compact undo native key"))?,
            ),
            slots,
            complete: b.state.execution_cohort_census_v15.compact_complete,
            old_native: p.before.map(|(_, v)| v.body),
            new_native: p.after.map(|(_, v)| v.body),
        });
        reserve_compact_undo_v15(b)?;
    }
    Ok(Some(p))
}
pub(super) fn prepare_compact_delta_v15(
    s: &PalwChainStateV2,
    table: u8,
    key: &[u8],
    new: Option<&[u8]>,
) -> Result<
    Option<crate::palw_native_resource_v1::NativeRetainedV1<CompactRootChangeV15>>,
    PalwStateV2Error,
> {
    let Some(p) = plan_compact_change_v15(s, table, key, new)? else {
        return Ok(None);
    };
    let lease = reserve_native_requests_v1(
        compact_growth_v15(p.absent).map_err(PalwStateV2Error::NativeResource)?,
    )
    .map_err(PalwStateV2Error::NativeResource)?;
    Ok(Some(
        crate::palw_native_resource_v1::NativeRetainedV1::from_owner_reserved_v1(p, lease),
    ))
}
/// Complete cloned-state delta application has no nested aux rollback; its
/// already-admitted inverse can physically remove a slot immediately.
pub(super) fn commit_compact_change_v15(s: &mut PalwChainStateV2, p: &CompactRootChangeV15) {
    let c = &mut s.execution_cohort_census_v15;
    if p.invalidate {
        c.compact_complete = false;
        return;
    }
    if p.before.map(|(k, _)| k) != p.after.map(|(k, _)| k) {
        if let Some((key, _)) = p.before {
            c.compact_by_root.remove(&key);
        }
    }
    if let Some((key, v)) = p.after {
        c.compact_by_root.insert(key, Some(v));
    }
}
/// No fallible operation follows the actual native mutation. Old physical map
/// nodes remain as tombstones through the scoped atomic interval.
pub(super) fn commit_compact_writer_v15(
    b: &mut TransitionBuilder<'_>,
    p: &CompactRootChangeV15,
    native_changed: bool,
) {
    if !native_changed {
        return;
    }
    if p.invalidate {
        if b.state.execution_cohort_census_v15.compact_complete {
            debug_assert!(b.compact_root_inactive_undo_v15.is_none());
            b.compact_root_inactive_undo_v15 = Some((p.ordinal, true));
            b.state.execution_cohort_census_v15.compact_complete = false;
        }
        return;
    }
    if let Some(undo) = p.undo {
        debug_assert!(b.compact_root_undo_v15.len() < b.compact_root_undo_v15.capacity());
        b.compact_root_undo_v15.push(undo);
    }
    let c = &mut b.state.execution_cohort_census_v15;
    if p.before.map(|(key, _)| key) != p.after.map(|(key, _)| key) {
        if let Some((key, _)) = p.before {
            *c.compact_by_root
                .get_mut(&key)
                .expect("prevalidated compact old node retained") = None;
        }
    }
    if let Some((key, pointer)) = p.after {
        c.compact_by_root.insert(key, Some(pointer));
    }
}
fn compact_undo_error_v15() -> crate::palw_claim_local_context_nodes_v2::ContextErrorV2 {
    crate::palw_claim_local_context_nodes_v2::ContextErrorV2::Malformed(
        "compact undo does not bind actual native journal",
    )
}
/// Exact scoped native inverse. No decoder, native reservation, global copy or
/// fallible reinsertion: every old node was preserved before the first write.
pub(super) fn rollback_compact_v15(
    b: &mut TransitionBuilder<'_>,
    first: usize,
) -> Result<(), crate::palw_claim_local_context_nodes_v2::ContextErrorV2> {
    let hash = |raw: &Option<Vec<u8>>| {
        raw.as_deref()
            .map(crate::palw_claim_local_v1::palw_liability_material_id_v1)
    };
    for undo in b
        .compact_root_undo_v15
        .iter()
        .rev()
        .take_while(|u| u.ordinal >= first)
    {
        let Some(PalwDeltaEntryV2::KernelRouteRow {
            table,
            key,
            old,
            new,
        }) = b.entries.get(undo.ordinal)
        else {
            return Err(compact_undo_error_v15());
        };
        if !compact_key_v15(*table, key)
            || &key[1..] != undo.id.as_byte_slice()
            || hash(old) != undo.old_native
            || hash(new) != undo.new_native
        {
            return Err(compact_undo_error_v15());
        }
        for slot in undo.slots.iter().flatten() {
            if !b
                .state
                .execution_cohort_census_v15
                .compact_by_root
                .contains_key(&slot.key)
            {
                return Err(compact_undo_error_v15());
            }
        }
    }
    if let Some((ordinal, _)) = b
        .compact_root_inactive_undo_v15
        .filter(|(ordinal, _)| *ordinal >= first)
    {
        if !matches!(b.entries.get(ordinal),Some(PalwDeltaEntryV2::KernelRouteRow{table,key,..})if compact_key_v15(*table,key))
        {
            return Err(compact_undo_error_v15());
        }
    }
    while b
        .compact_root_undo_v15
        .last()
        .is_some_and(|u| u.ordinal >= first)
    {
        let undo = b
            .compact_root_undo_v15
            .pop()
            .expect("checked compact undo suffix");
        for slot in undo.slots.into_iter().flatten().rev() {
            if slot.existed {
                *b.state
                    .execution_cohort_census_v15
                    .compact_by_root
                    .get_mut(&slot.key)
                    .expect("validated compact retained node") = slot.before;
            } else {
                b.state
                    .execution_cohort_census_v15
                    .compact_by_root
                    .remove(&slot.key);
            }
        }
        b.state.execution_cohort_census_v15.compact_complete = undo.complete;
    }
    if let Some((ordinal, complete)) = b.compact_root_inactive_undo_v15 {
        if ordinal >= first {
            b.state.execution_cohort_census_v15.compact_complete = complete;
            b.compact_root_inactive_undo_v15 = None;
        }
    }
    Ok(())
}
/// A complete fold/preview owns no outstanding internal aux rollback. Remove
/// only touched tombstones, O(actual S1 edits log N), then forget transient undo.
pub(super) fn finish_compact_observations_v15(b: &mut TransitionBuilder<'_>) {
    for undo in &b.compact_root_undo_v15 {
        for slot in undo.slots.iter().flatten() {
            if b.state
                .execution_cohort_census_v15
                .compact_by_root
                .get(&slot.key)
                == Some(&None)
            {
                b.state
                    .execution_cohort_census_v15
                    .compact_by_root
                    .remove(&slot.key);
            }
        }
    }
    b.compact_root_undo_v15.clear();
    b.compact_root_inactive_undo_v15 = None;
}
/// Explicit legacy/unbuilt diagnostic fallback; ordinary rollback uses exact undo.
pub(super) fn invalidate_compact_row_v15(s: &mut PalwChainStateV2, table: u8, key: &[u8]) {
    if compact_key_v15(table, key) {
        s.execution_cohort_census_v15.compact_complete = false;
    }
}
pub(super) fn prepare_compact_replay_activity_v15(
    s: &mut PalwChainStateV2,
    params: &PalwStateParamsV2,
    point: PalwBlockContextV2,
) {
    if !params.claim_local_settlement_active_at(point.daa_score) {
        s.execution_cohort_census_v15.compact_complete = false;
    }
}
pub(super) fn compact_shared_root_v15(
    s: &PalwChainStateV2,
    born: &PalwClaimStateV2,
) -> Result<Option<rewards::RewardReadinessV1>, PalwStateV2Error> {
    let c = &s.execution_cohort_census_v15;
    if !c.compact_complete {
        return Ok(None);
    }
    let lo = (born.execution_root, Hash64::default());
    let hi = (born.execution_root, Hash64::from_bytes([255; 64]));
    for ((root, id), slot) in c.compact_by_root.range(lo..=hi) {
        let Some(pointer) = slot else { continue };
        let _key = reserve_native_requests_v1(65).map_err(PalwStateV2Error::NativeResource)?;
        let mut key = Vec::with_capacity(65);
        key.push(b'S');
        key.extend_from_slice(id.as_byte_slice());
        let raw = s
            .kernel_route
            .as_ref()
            .and_then(|r| r.aux.get(&(PALW_CLAIM_LOCAL_TABLE_V1, key)))
            .ok_or(bad("compact indexed native row absent"))?;
        if crate::palw_claim_local_v1::palw_liability_material_id_v1(raw) != pointer.body {
            return Err(bad("compact indexed native hash differs"));
        }
        let row = super::palw_claim_local_compact_sources_v1::decode_checked_index_v15(raw, *id)?;
        if row.original.execution_root != *root || row.birth != pointer.point {
            return Err(bad("compact indexed native root/birth differs"));
        }
        if row.birth.blue_score <= born.accepted_blue_score
            && row.birth.daa_score <= born.accepted_daa
            && !row.permitted
        {
            return Ok(Some(rewards::RewardReadinessV1::Denied));
        }
    }
    Ok(Some(rewards::RewardReadinessV1::Ready))
}

/// Authenticate ALL current native sources in exactly this queried root,
/// including births after a historical consumer, against the genesis-rooted
/// CURRENT tally. Only the existing consumer visitor filters the proven set.
/// Never compare a birth-filtered subset with the current total.
pub(super) fn native_root_members_complete_v22(s:&PalwChainStateV2,root:Hash64,_consumer:PalwBlockContextV2)->Result<bool,PalwStateV2Error>{
 let c=&s.execution_cohort_census_v15;if !c.ready{return Ok(false)}
 let Some(population)=super::palw_claim_local_native_cohort_baseline_v22::current_native_root_population_v25(s,root)?else{return Ok(false)};
 let(lo,hi)=bounds(root);let mut count=0u64;
 for(key,m)in c.members.range(lo..=hi){
  if m.ambiguous||Key::of(key.id,&m.row)!=*key{return Ok(false)}
  if population.observed_v25().is_none_or(|p|key.birth.daa>p.daa_score||key.birth.blue>p.blue_score){return Ok(false)}
  // Stored accepted_block names the RED carrying block; it is NOT the
  // actual native accepting block. Stored identity is checked separately by
  // same_identity; authenticate the real accepting clock/fullpoint here.
  let exact=|point:PalwBlockContextV2|point.daa_score==key.birth.daa&&point.blue_score==key.birth.blue&&m.actual_birth.is_none_or(|actual|actual==point);
  let known=if let Some(source)=super::palw_claim_local_source_admission_v8::for_stored_claim_v8(s,key.id,&m.row)?{
   same_identity(source.original_claim(),&m.row)&&exact(source.accepting_point())
  }else if let Some(source)=super::palw_claim_local_exact_closed_source_v5::read_original_birth_v5(s,key.id,&m.row)?{
   same_identity(&source.original,&m.row)&&exact(source.birth)
  }else if let Some(source)=super::palw_claim_local_compact_sources_v1::read(s,key.id)?{
   same_identity(&source.original,&m.row)&&exact(source.birth)
  }else{super::palw_claim_local_execution_cohort_archive_v15::current_cohort_contains_member_v15(s,root,key.id,&m.row)?};
  if !known{return Ok(false)}count=count.checked_add(1).ok_or(bad("native canonical root source count overflow"))?;
 }
 Ok(count==population.total_v25())
}
