//! **The range twin reads what the element twin reads** (spec 04b §10.3, `palw_tir_fence2`): request by
//! request — every commit point of every corpus program, at every occurrence, at every position of
//! three layouts, every tile and scattered elements, in every mode the sizing asks (plain, both-mode,
//! history-only with and without the row pattern, a dissection's finalize, probes and bottoms) — the
//! two twins' units are the same sets (step leaves with their lanes and history marks, hypothetical
//! leaves, inventory leaves, location-free rows, the token, supplied elements, the row pattern), and
//! the worst closes they bound are the same, byte for byte. The work of both is printed.

#[path = "palw_tir_fixture_common.rs"]
mod fixture;
use fixture::*;

use std::collections::BTreeSet;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_step_refute::tiled_logits_scheme_id_v1;
use kaspa_consensus_core::palw_tir_class_v1::{
    PALW_TIR_CLASS_VERSION_V1, PALW_TIR_LAYOUT_VERSION_V1, PalwTirClassV1, PalwTirLayoutV1,
};
use kaspa_consensus_core::palw_tir_close_range_v1::{
    PalwTirCloseRangeRequestV1, PalwTirRangeCacheV1, PalwTirRangesV1, palw_tir_close_reads_range_split_v1,
    palw_tir_worst_closes_range_work_v1,
};
use kaspa_consensus_core::palw_tir_close_size_v1::{
    HistPattern, PalwTirCloseReadsV1, PalwTirCloseRequestV1, PalwTirCloseSizingV1, PalwTirParamFormV1, palw_tir_close_reads_split_v1,
    palw_tir_worst_closes_work_v1,
};
use kaspa_consensus_core::palw_tir_court_v1::PalwTirInventoryIndexV1;
use kaspa_consensus_core::palw_tir_step_v1::PalwTirStepSpaceV1;
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use misaka_palw_tir::TirProgramV1;
use misaka_palw_tir::demand::{DemandContext, history_length_v1};
use misaka_palw_tir::program::StateKind;

/// A class of `program` at a layout: `positions`, checkpoint interval `c`, `h_tile`, and commit tiles
/// from `tile(k)` (the logits node at `logits`).
struct Class {
    space: PalwTirStepSpaceV1,
    longest: PalwJobContextV2,
    inventory: PalwTirInventoryIndexV1,
}

fn class_at(program: &TirProgramV1, positions: u32, c: u32, h_tile: u32, tile: &dyn Fn(u32) -> u32, logits: u32) -> Option<Class> {
    let mut program = program.clone();
    program.logits_scheme_id.copy_from_slice(tiled_logits_scheme_id_v1().as_byte_slice());
    let bytes = program.encode();
    let program = TirProgramV1::decode_canonical(&bytes).ok()?;
    let mut commit_tiles = Vec::new();
    let mut k = 0u32;
    for (bi, b) in program.blocks.iter().enumerate() {
        for (ni, n) in b.nodes.iter().enumerate() {
            if n.commit {
                let is_logits = bi == program.schedule.post as usize && ni == program.logits as usize;
                commit_tiles.push(if is_logits { logits } else { tile(k) });
                k += 1;
            }
        }
    }
    let layout = PalwTirLayoutV1 {
        version: PALW_TIR_LAYOUT_VERSION_V1,
        max_context: positions,
        checkpoint_interval: c,
        h_tile,
        commit_tiles,
        state_tiles: program
            .states
            .iter()
            .enumerate()
            .map(|(j, s)| match s.kind {
                StateKind::Hist { .. } => 4 + (j as u32 % 3),
                StateKind::Fixed { .. } => 4 + (j as u32 % 5),
            })
            .collect(),
    };
    let class =
        PalwTirClassV1 { version: PALW_TIR_CLASS_VERSION_V1, program: bytes, layout, tokenizer_id: Hash64::from_bytes([3; 64]) };
    let space = PalwTirStepSpaceV1::new(&class).ok()?;
    let longest = kaspa_consensus_core::palw_tir_attempt_v1::palw_tir_canonical_context_v1(
        &class,
        class.class_id(&Hash64::from_bytes([0xA1; 64])),
        (1, positions),
    )?;
    let inventory = PalwTirInventoryIndexV1::new(&space.program)?;
    Some(Class { space, longest, inventory })
}

/// A read's units, normalized for comparison: `(reads, hist, supplied, pattern)`.
type Units = (PalwTirCloseReadsV1, PalwTirCloseReadsV1, BTreeSet<(u16, usize)>, HistPattern);

fn normalized(mut r: PalwTirCloseReadsV1) -> PalwTirCloseReadsV1 {
    r.loose_steps.sort_unstable();
    r
}

struct Probe<'a> {
    c: &'a Class,
    cache: PalwTirRangeCacheV1,
    requests: u64,
    element_work: u64,
    range_work: u64,
}

impl Probe<'_> {
    #[allow(clippy::too_many_arguments)]
    fn same(
        &mut self,
        what: &str,
        ctx: DemandContext,
        target: u16,
        elements: &[usize],
        supplied: &[u16],
        range: Option<(usize, usize)>,
        both: bool,
        hist_only: bool,
        pattern: bool,
    ) {
        let (space, job, inv) = (&self.c.space, &self.c.longest, &self.c.inventory);
        let e = palw_tir_close_reads_split_v1(
            space,
            job,
            inv,
            &PalwTirCloseRequestV1 { ctx, target, elements, supplied, range, both },
            u64::MAX,
            hist_only,
            pattern,
        );
        let ranges = PalwTirRangesV1::from_elements(elements);
        let r = palw_tir_close_reads_range_split_v1(
            space,
            job,
            inv,
            &mut self.cache,
            &PalwTirCloseRangeRequestV1 { ctx, target, ranges: &ranges, supplied, range, both },
            u64::MAX,
            hist_only,
            pattern,
        );
        self.requests += 1;
        match (e, r) {
            (Ok((mut reads, hist, pat, ew)), Ok(split)) => {
                self.element_work += ew;
                self.range_work += split.work;
                let supplied_e = std::mem::take(&mut reads.supplied);
                let e: Units = (normalized(reads), normalized(hist), supplied_e, pat);
                let supplied_r: BTreeSet<(u16, usize)> =
                    split.supplied.iter().flat_map(|(n, r)| r.iter_elements().map(move |x| (*n, x))).collect();
                let r: Units = (normalized(split.reads), normalized(split.hist), supplied_r, split.pattern);
                assert_eq!(e.0, r.0, "{what}: the reads differ");
                assert_eq!(e.1, r.1, "{what}: the history reads differ");
                assert_eq!(e.2, r.2, "{what}: the supplied elements differ");
                assert_eq!(e.3, r.3, "{what}: the row pattern differs");
            }
            (Err(_), Err(_)) => {}
            (e, r) => panic!("{what}: one twin refuses: element {:?}, range {:?}", e.err(), r.err()),
        }
    }
}

/// Every request the sizing can make, and more, on one class.
fn differential(name: &str, c: &Class) -> (u64, u64, u64) {
    let space = &c.space;
    let program = &space.program;
    let job = space.job_shape(&c.longest).expect("a job");
    let positions = job.positions;
    let mut p = Probe { c, cache: PalwTirRangeCacheV1::default(), requests: 0, element_work: 0, range_work: 0 };
    let occurrences = space.occurrences().to_vec();
    for (bi, block) in program.blocks.iter().enumerate() {
        for (ni, node) in block.nodes.iter().enumerate() {
            if !node.commit {
                continue;
            }
            let (bi8, ni16) = (bi as u8, ni as u16);
            let tile_len = space.commit_tile_len(bi8, ni16).expect("a tile") as usize;
            let reductions = kaspa_consensus_core::palw_tir_dissect_v1::palw_tir_cone_reductions_v1(block, ni16);
            for (o, (b, _)) in occurrences.iter().enumerate() {
                if *b != bi8 {
                    continue;
                }
                for pos in 0..positions {
                    if o == occurrences.len() - 1 && !job.runs_post(pos) {
                        continue;
                    }
                    let ctx = DemandContext { pos, occurrence: o as u16 };
                    let h = history_length_v1(&space.info, bi8, pos).expect("info");
                    let count: usize = node.out.resolve(h).iter().product();
                    let tiles: Vec<(usize, usize)> =
                        (0..count).step_by(tile_len).map(|first| (first, (first + tile_len).min(count))).collect();
                    let what =
                        |mode: &str, t: (usize, usize)| format!("{name} b{bi} n{ni} occ {o} pos {pos} [{}, {}) {mode}", t.0, t.1);
                    for (k, &(first, end)) in tiles.iter().enumerate() {
                        // Only every tile at the first positions, a sample after.
                        if pos >= 3 && k % 3 != (pos as usize % 3) {
                            continue;
                        }
                        let tile: Vec<usize> = (first..end).collect();
                        p.same(&what("plain", (first, end)), ctx, ni16, &tile, &[], None, false, false, false);
                        p.same(&what("both", (first, end)), ctx, ni16, &tile, &[], None, true, false, false);
                        p.same(&what("hist-only", (first, end)), ctx, ni16, &tile, &[], None, true, true, false);
                        p.same(&what("pattern", (first, end)), ctx, ni16, &tile, &[], None, true, true, true);
                        // Scattered elements: the ends of the tile and a stride.
                        let scattered: Vec<usize> = (first..end).step_by(3).chain(std::iter::once(end - 1)).collect();
                        p.same(&what("scattered", (first, end)), ctx, ni16, &scattered, &[], None, false, false, false);
                        if reductions.is_empty() {
                            continue;
                        }
                        // A dissection: the finalize, the probes at the first row, the bottoms.
                        let others: Vec<u16> = reductions.iter().copied().filter(|r| *r != ni16).collect();
                        p.same(&what("finalize", (first, end)), ctx, ni16, &tile, &others, None, false, false, false);
                        for r in &reductions {
                            let rh: usize = block.nodes[*r as usize].out.resolve(h).iter().product();
                            let es: Vec<usize> = (0..rh).step_by(2).collect();
                            let others: Vec<u16> = reductions.iter().copied().filter(|x| x != r).collect();
                            p.same(&what("probe", (first, end)), ctx, *r, &es, &others, Some((0, 1)), false, false, false);
                            let h_tile = space.layout.h_tile as usize;
                            for tau in [0usize, h.div_ceil(h_tile).saturating_sub(1)] {
                                let span = Some((tau * h_tile, ((tau + 1) * h_tile).min(h)));
                                p.same(&what("bottom", (first, end)), ctx, *r, &es, &others, span, true, false, false);
                            }
                        }
                    }
                }
            }
        }
    }
    (p.requests, p.element_work, p.range_work)
}

fn layouts() -> Vec<(&'static str, u32, u32, u32, fn(u32) -> u32, u32)> {
    vec![
        ("fixture", PREFILL + DECODE - 1, 2, 2, |k| 4 + (k * 7) % 6, 4096),
        ("mid", 24, 5, 4, |k| 4 + (k * 3) % 13, 1024),
        ("long", 40, 16, 8, |k| 16 << (k % 3), 512),
    ]
}

#[test]
fn the_range_twin_reads_what_the_element_twin_reads_request_by_request() {
    let mut total = 0u64;
    for (name, program, _, _) in programs() {
        for (lname, positions, c, h_tile, tile, logits) in layouts() {
            let positions = positions.min(program.history_bound);
            let Some(class) = class_at(&program, positions, c, h_tile, &tile, logits) else {
                eprintln!("{name} at {lname}: no class");
                continue;
            };
            let (requests, ew, rw) = differential(&format!("{name}@{lname}"), &class);
            eprintln!("{name:>28} at {lname:<7}: {requests:>6} requests the same; work element {ew:>10}, range {rw:>9}");
            total += requests;
        }
    }
    assert!(total > 10_000, "{total} requests compared");
}

/// **The same worst closes, byte for byte**, by both drivers — court on and off, both parameter
/// forms, every corpus program at every layout — with the work each did.
#[test]
fn the_range_twin_bounds_every_close_as_the_element_twin_does() {
    let mut compared = 0;
    for (name, program, _, _) in programs() {
        for (lname, positions, c, h_tile, tile, logits) in layouts() {
            let positions = positions.min(program.history_bound);
            let Some(class) = class_at(&program, positions, c, h_tile, &tile, logits) else { continue };
            for court in [false, true] {
                for form in [PalwTirParamFormV1::PerLeaf, PalwTirParamFormV1::Multiproof] {
                    let sizing = PalwTirCloseSizingV1 { form, court, cap: u64::MAX, stop_above: None };
                    let e = palw_tir_worst_closes_work_v1(&class.space, &class.inventory, &class.longest, &sizing);
                    let r = palw_tir_worst_closes_range_work_v1(&class.space, &class.inventory, &class.longest, &sizing);
                    match (e, r) {
                        (Ok((eb, ew)), Ok((rb, rw))) => {
                            assert_eq!(eb, rb, "{name}@{lname} court {court} {form:?}: the bounds differ");
                            if court && form == PalwTirParamFormV1::Multiproof {
                                eprintln!(
                                    "{name:>28} at {lname:<7}: {} commit points bound the same; work element {ew:>10}, range {rw:>9}",
                                    eb.len()
                                );
                            }
                            compared += 1;
                        }
                        (Err(a), Err(b)) => eprintln!("{name}@{lname} court {court} {form:?}: both refuse ({a} / {b})"),
                        (e, r) => panic!("{name}@{lname} court {court} {form:?}: one driver refuses: {:?} / {:?}", e.err(), r.err()),
                    }
                }
            }
        }
    }
    assert!(compared >= 40, "{compared} sizings compared");
}

/// **The cap binds the range twin as it binds the element twin**: a sizing given fewer steps than it
/// needs is refused by name, and one given exactly what it used is not.
#[test]
fn the_range_twin_stops_at_its_cap() {
    use kaspa_consensus_core::palw_tir_close_size_v1::PALW_TIR_CLOSE_SIZING_OVER_CAP_V1;
    for (name, program, _, _) in programs() {
        let Some(class) = class_at(&program, 24, 5, 4, &|k| 4 + (k * 3) % 13, 1024) else { continue };
        let sizing = |cap| PalwTirCloseSizingV1 { form: PalwTirParamFormV1::Multiproof, court: true, cap, stop_above: None };
        let Ok((bounds, work)) =
            palw_tir_worst_closes_range_work_v1(&class.space, &class.inventory, &class.longest, &sizing(u64::MAX))
        else {
            continue;
        };
        assert_eq!(
            palw_tir_worst_closes_range_work_v1(&class.space, &class.inventory, &class.longest, &sizing(work)).map(|x| x.0),
            Ok(bounds),
            "{name}: exactly its work admits"
        );
        assert_eq!(
            palw_tir_worst_closes_range_work_v1(&class.space, &class.inventory, &class.longest, &sizing(work - 1)),
            Err(PALW_TIR_CLOSE_SIZING_OVER_CAP_V1.to_string()),
            "{name}: one step less refuses"
        );
    }
}
