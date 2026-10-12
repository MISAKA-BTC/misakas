//! **SEAT-S4: a segment opening is authenticated against the claim before a partial seat compares**
//! (f1c_f1m spec §4-bis.10, S-4; `misaka_palw_base0::segment_opening`).
//!
//! The `SC01` opening a partial seat replayed carried the checkpoint, its chunks, the covered call,
//! the seed token, the leaf range and the committed leaf hashes, and the seat compared its replay
//! with `committed_leaf_hashes` — both sides from the opening. `SC02` carries the claim's binding,
//! the committed checkpoint with its opening, the claim's decode pin and a sibling path; the leaves
//! are the seat's own, and they must root to the claim's `step_merkle_root`.
//!
//! On the floor (dense, per-call checkpoints: genesis, resumed and seeded segments) and on the A16
//! held fixture (a fold at its retained level, the same fold at a level that cuts the segments'
//! edges, and a dense capture) every honest opening authenticates and replays to a match, and every
//! forged link is refused by name — the wrong segment or range, a proof that is not the segment's,
//! chunks that are not the committed checkpoint's (re-rooted or not), a counter off the cadence, a
//! seed the trace does not commit, siblings that do not root, a binding of another claim or another
//! job — and the self-consistent forgery `SC01` licensed (every served value the honest run's, the
//! claim a lie) replays to `matches == false`. No refusal is ever `Valid`.

mod common;

use common::*;
use kaspa_consensus_core::palw_attempt_v2::palw_attempt_job_v1;
use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use kaspa_consensus_core::palw_segment_resume_v1::{PalwSegmentClaimV1, PalwSegmentReplayV1};
use kaspa_consensus_core::palw_step::PalwShapeProfileV3;
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use kaspa_consensus_core::palw_verification_v2::{palw_segment_count_v2, palw_segment_leaf_range_v2};
use kaspa_hashes::Hash64;
use misaka_palw_base0::fp_capture::palw_base0_sparse_retain_level_for_class_v1;
use misaka_palw_base0::produce::base0_material_decode_any_v1;
use misaka_palw_base0::segment_opening::{
    Base0SegmentOpeningV2, Base0SegmentRefusalV1 as R, Base0SegmentSeedPinV1, base0_segment_opening_plan_v2,
};

/// One claim, as a partial seat meets it: the class, the job it derived, the claim's roots, and the
/// capture a server opens from.
struct Case<'a> {
    label: String,
    backend: &'a dyn PalwExecutionBackendV1,
    profile: PalwShapeProfileV3,
    job: PalwJobContextV2,
    prompt: Vec<usize>,
    capture: Vec<u8>,
    execution_root: Hash64,
    trace_root: Hash64,
    seats: u16,
    cap: u64,
}

impl Case<'_> {
    fn claim(&self, segment_index: u16) -> PalwSegmentClaimV1 {
        PalwSegmentClaimV1 { execution_root: self.execution_root, trace_root: self.trace_root, seat_count: self.seats, segment_index }
    }

    fn segments(&self) -> u16 {
        palw_segment_count_v2(self.seats)
    }

    fn open(&self, index: u16) -> Base0SegmentOpeningV2 {
        let bytes = self.backend.open_segment_checkpoint_v1(&self.capture, self.seats, index).expect("the producer opens its segment");
        assert_eq!(&bytes[..4], b"SC02", "{}: the authenticated form", self.label);
        Base0SegmentOpeningV2::decode_v2(&bytes).expect("decodes")
    }

    fn replay(&self, opening: &Base0SegmentOpeningV2, claim: PalwSegmentClaimV1) -> Result<PalwSegmentReplayV1, String> {
        self.backend.replay_segment_from_checkpoint_v1(&self.job, &self.prompt, &opening.encode_v2().expect("encodes"), claim)
    }

    /// The link named by the plan, and the backend verb never `Valid` on it.
    fn refused(&self, what: &str, opening: &Base0SegmentOpeningV2, claim: PalwSegmentClaimV1, expect: R) {
        assert_eq!(
            base0_segment_opening_plan_v2(opening, &self.profile, &self.job, claim, self.cap).err(),
            Some(expect.clone()),
            "{}: {what} — refused by the named link",
            self.label
        );
        match self.replay(opening, claim) {
            Err(why) => assert_eq!(why, expect.to_string(), "{}: {what}", self.label),
            Ok(replay) => panic!("{}: {what} replayed (matches = {}) instead of being refused", self.label, replay.matches),
        }
    }

    /// Every honest opening authenticates and replays to a match; returns them.
    fn honest_openings(&self) -> Vec<Base0SegmentOpeningV2> {
        (0..self.segments())
            .map(|index| {
                let opening = self.open(index);
                let replay =
                    self.replay(&opening, self.claim(index)).unwrap_or_else(|e| panic!("{} segment {index}: {e}", self.label));
                assert!(replay.matches, "{} segment {index}: the honest segment roots to the claim", self.label);
                assert_eq!((replay.window.leaf_start, replay.window.leaf_end), (opening.leaf_start, opening.leaf_end));
                opening
            })
            .collect()
    }

    /// Every forgery that needs no anchor, on segment `index`'s opening.
    fn assert_forgeries_refused(&self, index: u16, other: &Case<'_>) {
        let honest = self.open(index);
        let claim = self.claim(index);
        let k = self.segments();

        // The wrong segment: another index of the same cut, another cut of the same claim.
        if k > 1 {
            let other_index = (index + 1) % k;
            self.refused("another segment's opening", &self.open(other_index), claim, R::NotTheSeatsSegment);
        }
        self.refused(
            "another panel size's cut",
            &honest,
            PalwSegmentClaimV1 { seat_count: self.seats + 1, ..claim },
            R::NotTheSeatsSegment,
        );
        // The wrong range, either edge.
        let mut o = honest.clone();
        o.leaf_start += 1;
        self.refused("a range that is not the segment's", &o, claim, R::NotTheSegmentsRange);
        let mut o = honest.clone();
        o.leaf_end -= 1;
        self.refused("a range cut short", &o, claim, R::NotTheSegmentsRange);
        // A proof that is not the segment's.
        let mut o = honest.clone();
        o.proof.first_leaf_index += 1;
        self.refused("a proven range that is not the segment's span", &o, claim, R::ProofNotTheSegments);
        let mut o = honest.clone();
        o.proof.align_level = palw_base0_sparse_retain_level_for_class_v1(&self.profile, self.cap) + 1;
        self.refused("an alignment deeper than the class folds at", &o, claim, R::ProofNotTheSegments);
        let mut o = honest.clone();
        o.proof.siblings.push(Hash64::from_u64_word(0x5B));
        self.refused("a path longer than the range's", &o, claim, R::ProofPathNotTheRanges);
        // Siblings that do not root: authenticated, replayed, and not a match.
        if !honest.proof.siblings.is_empty() {
            let mut o = honest.clone();
            o.proof.siblings[0] = Hash64::from_u64_word(0x5B);
            let replay = self.replay(&o, claim).expect("a well-shaped path replays");
            assert!(!replay.matches, "{}: siblings that are not the tree's do not root to the claim", self.label);
        }
        // A binding that is not the claim's, the seat's job's or its own.
        self.refused(
            "a claim whose execution root is another's",
            &honest,
            PalwSegmentClaimV1 { execution_root: Hash64::from_u64_word(0xE0), ..claim },
            R::NotTheClaimsExecution,
        );
        self.refused(
            "a claim whose trace root is another's",
            &honest,
            PalwSegmentClaimV1 { trace_root: Hash64::from_u64_word(0x70), ..claim },
            R::NotTheClaimsTrace,
        );
        self.refused("another claim's opening (a borrowed binding)", &other.open(index), claim, R::NotTheClaimsExecution);
        let mut o = honest.clone();
        o.binding.step_merkle_root = Hash64::from_u64_word(0x51);
        self.refused("a binding that does not rebuild its root", &o, claim, R::BindingDoesNotVerify);
        let wrong_job = PalwJobContextV2 { job_nullifier: Hash64::from_u64_word(0x0A), ..self.job.clone() };
        assert_eq!(
            base0_segment_opening_plan_v2(&honest, &self.profile, &wrong_job, claim, self.cap).err(),
            Some(R::NotTheSeatsJob),
            "{}: the seat's own job is the question",
            self.label
        );
        assert!(
            self.backend.replay_segment_from_checkpoint_v1(&wrong_job, &self.prompt, &honest.encode_v2().unwrap(), claim).is_err()
        );
        // A seed pin where none is read.
        if honest.seed_pin.is_none() {
            let mut o = honest.clone();
            let ids = base0_material_decode_any_v1(&self.capture).expect("decodes").generated_token_ids().to_vec();
            o.seed_pin = Some(Base0SegmentSeedPinV1::Tiled(kaspa_consensus_core::palw_step_refute::PalwTiledDecodeTokensV1 {
                rows_root: Hash64::default(),
                generated_token_ids: ids,
            }));
            self.refused("a decode pin an opening inside the prefill does not read", &o, claim, R::SeedPinUnexpected);
        }
        // A genesis replay reads the prompt: another prompt is not the job's.
        if honest.anchor.is_none() {
            let mut prompt = self.prompt.clone();
            prompt[0] = (prompt[0] + 1) % self.profile.vocab_size as usize;
            let got = self.backend.replay_segment_from_checkpoint_v1(&self.job, &prompt, &honest.encode_v2().unwrap(), claim);
            assert_eq!(got.err(), Some(R::PromptNotTheJobs.to_string()), "{}: another prompt", self.label);
        }
    }

    /// The forgeries of a resumed opening's anchor and seed.
    fn assert_anchor_forgeries_refused(&self, opening: &Base0SegmentOpeningV2, index: u16) {
        let claim = self.claim(index);
        let anchor = opening.anchor.as_ref().expect("a resumed opening");
        // Chunks that are not the committed checkpoint's state.
        let mut o = opening.clone();
        o.anchor.as_mut().unwrap().chunks[0][0] ^= 1;
        self.refused("a flipped state byte", &o, claim, R::AnchorNotCommitted);
        // The same, re-rooted: the leaf re-derived from the forged state and its opening's hash moved
        // with it — the path is still the honest one, so the leg does not open.
        let mut o = opening.clone();
        {
            let a = o.anchor.as_mut().unwrap();
            a.chunks[0][0] ^= 1;
            let positions = kaspa_consensus_core::palw_context_ladder::palw_checkpoint_positions_at_v1(
                &self.profile,
                &self.job,
                a.leaf.covered_decode_call,
            );
            a.leaf.state_chunks_root =
                misaka_palw_base0::fp_interval::base0_state_chunks_root_for_v1(&self.profile, positions, &a.chunks)
                    .expect("the forged state has a root");
            a.opening.leaf_hash = kaspa_consensus_core::palw_step_leg::checkpoint_leaf_hash_v2(
                &self.job.context_hash(),
                &opening.binding.checkpoint_profile.profile_hash(),
                &opening.binding.state_chunk_map_id,
                &a.leaf,
            );
        }
        self.refused("a forged state with its leaf re-derived", &o, claim, R::AnchorNotCommitted);
        let mut o = opening.clone();
        o.anchor.as_mut().unwrap().leaf.covered_decode_call += 1;
        self.refused("a counter off the cadence", &o, claim, R::AnchorNotCanonical);
        let mut o = opening.clone();
        o.anchor.as_mut().unwrap().chunks.clear();
        self.refused("a named checkpoint with no state", &o, claim, R::AnchorCarriesNoState);
        // The seed: the claim's pin, with the consumed id moved, and without it.
        if let Some(pin) = &opening.seed_pin {
            let prefill = u64::from(self.job.declared_prefill_tokens);
            let positions = kaspa_consensus_core::palw_context_ladder::palw_checkpoint_positions_at_v1(
                &self.profile,
                &self.job,
                anchor.leaf.covered_decode_call,
            );
            let at = (u64::from(positions) - prefill) as usize;
            let mut o = opening.clone();
            match o.seed_pin.as_mut().unwrap() {
                Base0SegmentSeedPinV1::Flat(p) => {
                    p.generated_token_ids[at] = (p.generated_token_ids[at] + 1) % self.profile.vocab_size
                }
                Base0SegmentSeedPinV1::Tiled(p) => {
                    p.generated_token_ids[at] = (p.generated_token_ids[at] + 1) % self.profile.vocab_size
                }
            }
            self.refused("a seed the trace does not commit", &o, claim, R::SeedPinNotCommitted);
            let mut o = opening.clone();
            o.seed_pin = None;
            self.refused("a resume at a decode call without the pin", &o, claim, R::SeedPinMissing);
            assert!(matches!(pin, Base0SegmentSeedPinV1::Flat(_) | Base0SegmentSeedPinV1::Tiled(_)));
        }
    }

    /// **The claim is a lie at `leaf`** — the drill's corruption, committed by the backend's own
    /// fault path — and the producer serves (a) its own opening, (b) the honest run's opening under
    /// the lying claim, (c) the honest run's every served value with only the binding the claim's
    /// (the `SC01` self-consistent forgery: leaves, chunks and seed all the honest recompute).
    /// (a) matches exactly where the proven range misses the lie; (b) is refused; (c) never matches
    /// the segment that holds the lie.
    fn assert_a_lie_is_not_licensed(&self, leaf: u64) {
        let lie = self.backend.execute_with_injected_fault(&self.job, &self.prompt, leaf).expect("the drill commits its lie");
        assert_ne!(lie.execution_root, self.execution_root, "{}: the lie moved the commitment", self.label);
        let lying = Case {
            capture: lie.material.clone(),
            execution_root: lie.execution_root,
            trace_root: lie.trace_root,
            label: format!("{} (lie at {leaf})", self.label),
            ..self.shallow()
        };
        let leaf_count = base0_material_decode_any_v1(&lie.material).expect("decodes").binding().step_leaf_count;
        for index in 0..self.segments() {
            let own = lying.open(index);
            let (first, end) = (own.proof.first_leaf_index, own.proof.first_leaf_index + own.proof.leaf_count);
            let replay = lying.replay(&own, lying.claim(index)).expect("the liar's own opening authenticates");
            assert_eq!(replay.matches, !(first..end).contains(&leaf), "{} segment {index} [{first}, {end})", lying.label);
            let honest = self.open(index);
            lying.refused("the honest opening under the lying claim", &honest, lying.claim(index), R::NotTheClaimsExecution);
            let mut forged = honest.clone();
            forged.binding = own.binding.clone();
            // Every served value the honest run's, the binding the claim's: whichever segment it is,
            // one path node or one leaf covers the lie, so the honest values root to the honest tree.
            let (s, e) = palw_segment_leaf_range_v2(leaf_count, self.segments(), index).unwrap();
            let replay = lying.replay(&forged, lying.claim(index)).expect("every link but the leaves is the claim's");
            assert!(!replay.matches, "{}: the self-consistent forgery licensed segment {index} [{s}, {e})", lying.label);
        }
    }

    fn shallow(&self) -> Case<'_> {
        Case {
            label: self.label.clone(),
            backend: self.backend,
            profile: self.profile.clone(),
            job: self.job.clone(),
            prompt: self.prompt.clone(),
            capture: self.capture.clone(),
            execution_root: self.execution_root,
            trace_root: self.trace_root,
            seats: self.seats,
            cap: self.cap,
        }
    }
}

fn attempt_case<'a>(
    label: &str,
    backend: &'a dyn PalwExecutionBackendV1,
    profile: &PalwShapeProfileV3,
    anchor: u64,
    draw: bool,
    seats: u16,
    cap: u64,
) -> Case<'a> {
    let (canonical, prompt) = backend.job_for_anchor(Hash64::from_u64_word(anchor)).expect("the anchor implies a job");
    let job = palw_attempt_job_v1(canonical, draw);
    let out = backend.execute(&job, &prompt).expect("the attempt runs");
    Case {
        label: label.to_string(),
        backend,
        profile: profile.clone(),
        job,
        prompt,
        capture: out.material,
        execution_root: out.execution_root,
        trace_root: out.trace_root,
        seats,
        cap,
    }
}

#[test]
fn the_floor_authenticates_every_segment_link_and_refuses_each_forgery() {
    let backend = floor_backend(PalwPromptIdsFormV1::MerkleV1);
    let cap = backend.step_ladder_cap();
    let profile = backend.profile().clone();
    // The floor's canonical job is mostly prefill; the cut fine enough for a segment to start in a
    // decode call after a committed checkpoint is the one a large panel draws.
    let mut chosen = None;
    for seats in [5u16, 9, 17, 33] {
        let case = attempt_case(&format!("floor {seats} seats"), &backend, &profile, 0x5E6_0001, false, seats, cap);
        let openings = case.honest_openings();
        if openings.iter().any(|o| o.anchor.is_some()) {
            chosen = Some((case, openings));
            break;
        }
    }
    let (case, openings) = chosen.expect("a panel of at most 33 seats cuts a segment after a committed checkpoint");
    let other = attempt_case("floor other", &backend, &profile, 0x5E6_0002, false, case.seats, cap);
    // The floor's canonical job resumes: at least one segment from a committed checkpoint at a
    // decode call, consuming a seed read off the claim's flat pin.
    let resumed: Vec<(u16, &Base0SegmentOpeningV2)> =
        openings.iter().enumerate().filter(|(_, o)| o.anchor.is_some()).map(|(i, o)| (i as u16, o)).collect();
    assert!(!resumed.is_empty(), "a segment of the floor resumes from a committed checkpoint");
    assert!(resumed.iter().any(|(_, o)| matches!(o.seed_pin, Some(Base0SegmentSeedPinV1::Flat(_)))), "and one consumes a pinned seed");
    assert!(openings.iter().any(|o| o.anchor.is_none()), "and the first replays from the prompt");
    for opening in &openings {
        assert_eq!(opening.proof.align_level, 0, "a dense capture proves its segment exactly");
        eprintln!(
            "floor segment {}: [{}, {}) {} anchor, {} pin, {} siblings — {} B",
            opening.segment_index,
            opening.leaf_start,
            opening.leaf_end,
            if opening.anchor.is_some() { "resumed" } else { "genesis" },
            if opening.seed_pin.is_some() { "a" } else { "no" },
            opening.proof.siblings.len(),
            opening.encode_v2().unwrap().len()
        );
    }
    for index in 0..case.segments() {
        case.assert_forgeries_refused(index, &other);
    }
    for (index, opening) in &resumed {
        case.assert_anchor_forgeries_refused(opening, *index);
    }
    // An anchor past the range: the last committed checkpoint served for the first resumed segment.
    let (index, opening) = resumed[0];
    let last = openings.iter().filter_map(|o| o.anchor.clone()).max_by_key(|a| a.leaf.covered_decode_call).unwrap();
    if last.leaf.covered_decode_call > opening.anchor.as_ref().unwrap().leaf.covered_decode_call {
        let mut o = opening.clone();
        o.anchor = Some(last);
        case.refused("a checkpoint after the range's first leaf", &o, case.claim(index), R::AnchorPastTheRange);
    }
    let leaf_count = base0_material_decode_any_v1(&case.capture).unwrap().binding().step_leaf_count;
    for leaf in [3, leaf_count / 2, leaf_count - 2] {
        case.assert_a_lie_is_not_licensed(leaf);
    }
}

#[test]
fn the_a16_held_fold_authenticates_at_its_retained_level_and_refuses_each_forgery() {
    use kaspa_consensus_core::palw_qwen25_profile::{qwen25_a16_held_canonical_v1, qwen25_a16_profile_v7};
    use kaspa_consensus_core::palw_resource_profile_v1::palw_attempt_capture_folds_v1;
    let artifact = a16_artifact(128);
    let held = qwen25_a16_profile_v7(a16_geometry(128)).expect("the held row");
    assert!(palw_attempt_capture_folds_v1(&held));
    let backend = a16_backend(&artifact, &held, qwen25_a16_held_canonical_v1(held.n_ctx));
    let cap = backend.step_ladder_cap();
    let fold = attempt_case("A16 held fold", &backend, &held, 0x5E6_A001, false, 5, cap);
    let other = attempt_case("A16 held fold other", &backend, &held, 0x5E6_A002, false, 5, cap);
    let retained = misaka_palw_base0::produce::base0_fp_material_decode_v2(&fold.capture).expect("a held attempt is a fold");
    let openings = fold.honest_openings();
    for opening in &openings {
        assert_eq!(opening.proof.align_level, retained.step_tree.retain_level(), "a fold proves at its retained level");
        assert!(opening.anchor.is_none() && opening.seed_pin.is_none(), "a held fold keeps no state: genesis");
    }
    for index in 0..fold.segments() {
        fold.assert_forgeries_refused(index, &other);
    }

    // The same execution folded at level 3 — the segments' edges now cut its blocks, so the
    // proven span is wider than the segment on both sides and the seat folds partial edges.
    let plan = misaka_palw_base0::engine_a16::A16Engine::new(&artifact).unwrap().plan_from_profile(&held).unwrap();
    let mut dense = misaka_palw_base0::qwen25_a16_backend::a16_execute_for_attempt_capped_v1(
        &artifact,
        &held,
        Some(&plan),
        &fold.job,
        &fold.prompt,
        cap,
    )
    .expect("the dense capture of the same job");
    assert_eq!(dense.execution_root, fold.execution_root, "one execution, two retentions");
    let dense_capture = misaka_palw_base0::produce::base0_material_encode_v1(&dense).expect("encodes");
    dense.step_tree =
        Some(misaka_palw_base0::fp_capture::Base0SparseStepTreeV1::from_leaves_capped_v1(&dense.tiles.leaves, 3, cap).expect("folds"));
    let ids: Vec<u32> = fold.prompt.iter().map(|t| *t as u32).collect();
    let level3 = Case {
        capture: misaka_palw_base0::produce::base0_fp_material_encode_v2(&dense, &ids).expect("the fold retains"),
        label: "A16 held fold@3".into(),
        ..fold.shallow()
    };
    let mut widened = 0;
    for seats in [5u16, 6, 7, 8] {
        let level3 = Case { seats, label: format!("A16 held fold@3, {seats} seats"), ..level3.shallow() };
        for opening in &level3.honest_openings() {
            assert_eq!(opening.proof.align_level, 3);
            let (first, end) = (opening.proof.first_leaf_index, opening.proof.first_leaf_index + opening.proof.leaf_count);
            assert!(first <= opening.leaf_start && opening.leaf_end <= end && first % 8 == 0);
            assert!(opening.leaf_start - first < 8 && end - opening.leaf_end < 8, "at most a block past either edge");
            widened += usize::from((first, end) != (opening.leaf_start, opening.leaf_end));
        }
    }
    assert!(widened > 0, "some segment's edge cuts a block, and the seat folds the partial edges");
    for index in 0..level3.segments() {
        level3.assert_forgeries_refused(index, &other);
    }

    // The dense capture of the held class proves each segment exactly, and a lie in it is caught
    // in exactly the segment that holds it.
    let exact = Case { capture: dense_capture, label: "A16 held dense".into(), ..fold.shallow() };
    for opening in exact.honest_openings() {
        assert_eq!(opening.proof.align_level, 0);
    }
    exact.assert_forgeries_refused(1, &other);
    let leaf_count = retained.binding.step_leaf_count;
    for leaf in [2, leaf_count / 2 + 1, leaf_count - 1] {
        exact.assert_a_lie_is_not_licensed(leaf);
    }
}

/// **What an opening weighs at the t12 held rows** (A16 graph-v7 at 8,192 and 2,097,152 positions,
/// their canonical held jobs) — the binding (the profile rides whole), the path of the segment's
/// span at level 12, and the digests a seat folds; five seats (four segments) and two.
#[test]
fn segment_opening_sizes_at_the_t12_held_rows() {
    use kaspa_consensus_core::palw_qwen25_profile::{QWEN25_1_5B, qwen25_a16_artifact_row_profile_v7, qwen25_a16_held_canonical_v1};
    use kaspa_consensus_core::palw_state_chunk_map::{PALW_HELD_STEP_LADDER_V1, integer_kv_checkpoint_profile_v1};
    use kaspa_consensus_core::palw_step_leg::{PALW_STEP_LEG_OBJECT_VERSION_V1, PalwStepBindingV2};
    use misaka_palw_base0::fp_capture::{PALW_BASE0_SPARSE_RETAIN_LEVEL_V1, base0_range_sibling_count_v1};
    for n_ctx in [8_192u32, 2_097_152] {
        let geometry = kaspa_consensus_core::palw_qwen25_profile::PalwQwen25GeometryV1 { n_ctx, ..QWEN25_1_5B };
        let profile = qwen25_a16_artifact_row_profile_v7(geometry).expect("the held row");
        let (prefill, decode) = qwen25_a16_held_canonical_v1(n_ctx);
        let ctx = kaspa_consensus_core::palw_base0_profile::rc_job_context(&profile, prefill, decode);
        let leaves =
            kaspa_consensus_core::palw_step::step_leaf_count_capped_v1(&profile, &ctx, PALW_HELD_STEP_LADDER_V1).expect("prices");
        let binding = PalwStepBindingV2 {
            version: PALW_STEP_LEG_OBJECT_VERSION_V1,
            job_context: ctx.clone(),
            shape_profile: profile.clone(),
            checkpoint_profile: integer_kv_checkpoint_profile_v1(1),
            state_chunk_map_id: profile.state_chunk_map_id,
            full_logits_trace_root: Hash64::default(),
            activation_leg_root: Hash64::default(),
            step_leaf_count: leaves,
            step_merkle_root: Hash64::default(),
            checkpoint_count: prefill + decode - 1,
            checkpoint_merkle_root: Hash64::default(),
            committed_execution_root: Hash64::default(),
        };
        let binding_bytes = borsh::to_vec(&binding).unwrap().len();
        assert_eq!(palw_base0_sparse_retain_level_for_class_v1(&profile, PALW_HELD_STEP_LADDER_V1), PALW_BASE0_SPARSE_RETAIN_LEVEL_V1);
        let block = 1u64 << PALW_BASE0_SPARSE_RETAIN_LEVEL_V1;
        for seats in [5u16, 2] {
            let k = palw_segment_count_v2(seats);
            let mut worst = 0usize;
            let mut digests = 0u64;
            for index in 0..k {
                let (s, e) = palw_segment_leaf_range_v2(leaves, k, index).unwrap();
                let (first, end) = (s - s % block, (e.div_ceil(block) * block).min(leaves));
                let siblings = base0_range_sibling_count_v1(leaves, first, end - first).unwrap();
                let opening = Base0SegmentOpeningV2 {
                    version: misaka_palw_base0::segment_opening::PALW_SEGMENT_OPENING_VERSION_V2,
                    segments: k,
                    segment_index: index,
                    leaf_start: s,
                    leaf_end: e,
                    binding: binding.clone(),
                    proof: misaka_palw_base0::segment_opening::Base0SegmentProofV1 {
                        align_level: PALW_BASE0_SPARSE_RETAIN_LEVEL_V1,
                        first_leaf_index: first,
                        leaf_count: end - first,
                        siblings: vec![Hash64::default(); siblings],
                    },
                    anchor: None,
                    seed_pin: None,
                };
                let bytes = opening.encode_v2().expect("encodes").len();
                worst = worst.max(bytes);
                digests = digests.max((end - first) / block);
                assert!(first <= s && e <= end && s - first < block && end - e < block, "at most a block past either edge");
            }
            eprintln!(
                "A16 held v7 @ {n_ctx}: {leaves} leaves, {seats} seats ({k} segments): binding {binding_bytes} B, largest opening {worst} B, \
                 largest seat fold {digests} digests ({} KiB), extension ≤ 2 × {block} leaves",
                digests * 64 / 1024
            );
            assert!(worst < 64 << 10, "an opening at {n_ctx} is kilobytes, far inside the 4 MiB lane: {worst}");
        }
    }
}

/// **The sampled-site check (S3) replays the site's segment from the capture's own authenticated
/// opening** — a held attempt's fold and its dense capture agree with themselves; a capture whose
/// committed tile at the site is a lie does not.
#[test]
fn a_sampled_site_replays_from_the_captures_authenticated_opening() {
    use kaspa_consensus_core::palw_layer_sample_v3::PalwLayerSiteV3;
    use kaspa_consensus_core::palw_qwen25_profile::{qwen25_a16_held_canonical_v1, qwen25_a16_profile_v7};
    use kaspa_consensus_core::palw_step::{PalwStepCoordinateV1, PalwStepTableV1, canonical_step_leaf_index};
    let artifact = a16_artifact(128);
    let held = qwen25_a16_profile_v7(a16_geometry(128)).expect("the held row");
    let backend = a16_backend(&artifact, &held, qwen25_a16_held_canonical_v1(held.n_ctx));
    let case = attempt_case("A16 held site", &backend, &held, 0x5E6_5173, false, 5, backend.step_ladder_cap());
    for site in [PalwLayerSiteV3 { layer: 0, position: 0 }, PalwLayerSiteV3 { layer: 1, position: 3 }] {
        assert_eq!(backend.replay_layer_site_v3(&case.capture, site, 5), Ok(true), "{site:?}: the fold agrees with itself");
        let node_slot = held.global_node_slot(PalwStepTableV1::Attn, site.layer, 0).expect("a node");
        let coord = PalwStepCoordinateV1 { call_index: 0, node_slot, position: site.position, tile_index: 0 };
        let leaf = canonical_step_leaf_index(&held, &case.job, &coord).expect("a main leaf");
        let lie = backend.execute_with_injected_fault(&case.job, &case.prompt, leaf).expect("the drill commits its lie");
        assert_eq!(backend.replay_layer_site_v3(&lie.material, site, 5), Ok(false), "{site:?}: a lie at the site is caught");
    }
}
