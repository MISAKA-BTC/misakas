//! **ADR-0170 (`palw_anchor_window_v1`) on ADR-0147's admission jury, through the fold.**
//!
//! The fixture is ADR-0147's contested network (`to_first_audit`'s): ten-DAA spans, an audit every hundred spans (span 100, DAA 1,000),
//! possession proofs for span 98 at DAA 985, Kimi a `Candidate` the network's jury must find a majority holding. What changes is where the
//! jury's randomness — the seed anchor — comes from:
//!
//! * **M1** — the ONLY admitted attempts of the window are MERGED ones (the shape of the int-11 drill's head class, whose audit span found
//!   two heartbeats and no attempt in the span before): the audit seats past the fence, and does not below it;
//! * **M2** — an anchor recorded long before the audit survives the span boundaries between (below the fence span 98's opening cleared it);
//! * **M3** — the window is `S − 24 … S − 1`: an anchor of span 76 (24 back) is read, one of span 75 is not; and the jury's population is
//!   cut at the ANCHOR's span, so a bond registered after the seed existed is not on the jury it seeds.
//!
//! And before the fence's height every fold is byte-identical to a chain with no fence at all.

use super::*;
use crate::palw_admission_v2::PalwAdmissionParamsV2;
use crate::palw_model_registry_v1::PalwModelLifecycleV1;

/// The chain a fixture walks: the fenced (or unfenced) params, the registry fold, and a counter that keeps block words and blue scores rising.
struct Walk<'a> {
    p: PalwStateParamsV2,
    f: &'a PalwModelRegistryFoldV1,
    state: PalwChainStateV2,
    word: u64,
    roots: Vec<Hash64>,
}

fn floor_attempt(nonce: u64) -> PalwAttemptEnvelopeV2 {
    attempt_for_class(40, nonce, h64(1), bond_key(1), vec![7; 4], op_id(21), h64(11))
}

impl<'a> Walk<'a> {
    fn new(p: PalwStateParamsV2, f: &'a PalwModelRegistryFoldV1) -> Self {
        Walk { p, f, state: PalwChainStateV2::genesis(), word: 0, roots: Vec::new() }
    }

    fn next_ctx(&mut self, daa: u64) -> PalwBlockContextV2 {
        self.word += 1;
        ctx(self.word, daa, self.word)
    }

    fn extras(&self) -> PalwTransitionExtrasV1 {
        armed(Some(self.f.clone()))
    }

    fn keep(&mut self, state: PalwChainStateV2) {
        self.roots.push(state.state_root());
        self.state = state;
    }

    /// A block at `daa` carrying `objects` and no work.
    fn block(&mut self, daa: u64, objects: &[PalwConsensusObjectV2]) -> &mut Self {
        let c = self.next_ctx(daa);
        let (next, _) = fold_step(&self.state, &self.p, &c, objects, None, &self.extras()).expect("the block folds");
        self.keep(next);
        self
    }

    /// A block at `daa` whose OWN work is an admitted floor attempt — the first nonce the fold takes.
    fn own_anchor(&mut self, daa: u64) -> &mut Self {
        let c = self.next_ctx(daa);
        for nonce in 1_000..1_200 {
            if let Ok((next, _)) = fold_step(&self.state, &self.p, &c, &[], Some(&floor_attempt(nonce)), &self.extras()) {
                self.keep(next);
                return self;
            }
        }
        panic!("no floor attempt admits at daa {daa}")
    }

    /// A block at `daa` with no work of its own that MERGES an admitted floor attempt (the attempt's block is `carrying`, its execution key
    /// `key`) — what a heartbeat does when it takes a REAL attempt (blue or red) into its mergeset.
    fn merged_anchor(&mut self, daa: u64, carrying: BlockHash, key: Hash64) -> &mut Self {
        let c = self.next_ctx(daa);
        let admission = PalwAdmissionParamsV2::new(500).unwrap();
        for nonce in 2_000..2_200 {
            let env = floor_attempt(nonce);
            let works = [PalwMergedWorkV1 {
                carrying_block: carrying,
                work: PalwBlockWorkV3::Attempt(&env),
                execution_key: key,
                subsidy: 0,
                escrow_carve: None,
                bits: 0,
                job_anchor: Hash64::default(),
            }];
            if let Ok((next, _, skips)) = apply_palw_transition_v7(
                &self.state,
                &self.p,
                Some(&admission),
                &c,
                &[],
                PalwBlockWorkV3::None,
                &works,
                Hash64::default(),
                false,
                false,
                false,
                false,
                &self.extras(),
            ) && skips.is_empty()
            {
                next.assert_internal_consistency(&self.p).expect("internal consistency after apply");
                self.keep(next);
                return self;
            }
        }
        panic!("no merged floor attempt admits at daa {daa}")
    }

    fn kimi(&self) -> PalwModelLifecycleV1 {
        self.state.model_lifecycle(&kimi_id()).expect("Kimi has a row").state
    }
}

fn on() -> PalwStateParamsV2 {
    params().with_anchor_window_from_daa(Some(0))
}

fn far() -> PalwStateParamsV2 {
    params().with_anchor_window_from_daa(Some(1_000_000_000))
}

/// An anchor event: a block whose own work is an admitted attempt, at `daa`.
fn own_at(daa: u64) -> impl Fn(&mut Walk<'_>) {
    move |walk| {
        walk.own_anchor(daa);
    }
}

/// An anchor event: heartbeats, then a block at `daa` that merges an admitted attempt (the attempt's block `carrying`, its key `key`).
fn merged_at(daa: u64, carrying: BlockHash, key: Hash64) -> impl Fn(&mut Walk<'_>) {
    move |walk| {
        walk.block(daa - 5, &[]).merged_anchor(daa, carrying, key);
    }
}

/// No anchor event at all: two heartbeats.
fn heartbeats_only(walk: &mut Walk<'_>) {
    walk.block(990, &[]).block(995, &[]);
}

/// The contested network at DAA 100, possession proofs for span 98 from every operator at DAA 985, and `anchor` — folded at its own DAA —
/// before or after them; the audit block (span 100, DAA 1,000) last. Returns the walk.
fn to_audit<'a>(
    p: PalwStateParamsV2,
    f: &'a PalwModelRegistryFoldV1,
    root: Hash64,
    operands: &[crate::palw_artifact::PalwArtifactOperandV1],
    anchor_at: u64,
    anchor: &dyn Fn(&mut Walk<'a>),
) -> Walk<'a> {
    // (the anchor event is the caller's own blocks at its own DAA: folded between the class registration and the proofs when it is before DAA
    // 985, between the proofs and the audit otherwise — block DAAs only rise)
    let proofs: Vec<PalwConsensusObjectV2> = SYBILS.chain(HONEST).map(|n| proof(operands, bond_key(n), 98)).collect();
    let mut walk = Walk::new(p, f);
    let c = walk.next_ctx(100);
    let (s1, _) = fold_step(&walk.state, &walk.p, &c, &contested_network(root, false), None, &armed(None)).unwrap();
    walk.keep(s1);
    if anchor_at < 985 {
        anchor(&mut walk);
    }
    walk.block(985, &proofs);
    if anchor_at >= 985 {
        anchor(&mut walk);
    }
    walk.block(1_000, &[]);
    walk
}

#[test]
fn m1_the_audit_seats_when_the_only_admitted_attempts_in_the_window_are_merged_and_not_below_the_fence() {
    let (operands, root) = inventory();
    let f = fold(kimi_work());
    // The int-11 drill's shape: span 99 holds heartbeats and the one attempt there is is MERGED (a REAL attempt a heartbeat takes in).
    let merged = merged_at(995, block(0x7E), h64(0xAE));
    let fenced = to_audit(on(), &f, root, &operands, 995, &merged);
    let anchor = *fenced.state.round_seed_anchor().expect("the merged attempt anchored");
    assert_eq!((anchor.block, anchor.execution_key), (block(0x7E), h64(0xAE)));
    // (the audit block is the walk's last: it read the anchor of span 99, and it is still there)
    assert_eq!(fenced.kimi(), PalwModelLifecycleV1::Prefetching, "the audit seats on a merged anchor past the fence");

    // Below the fence the same blocks leave nothing to seed the jury with: the class stays a Candidate — the drill's skipped audit.
    let below = to_audit(params(), &f, root, &operands, 995, &merged);
    assert!(below.state.round_seed_anchor().is_none(), "a merged attempt is no anchor below the fence");
    assert_eq!(below.kimi(), PalwModelLifecycleV1::Candidate, "no anchor, no jury, no audit");

    // And with no attempt in the window at all, the fence changes nothing: still no jury.
    let none = to_audit(on(), &f, root, &operands, 995, &heartbeats_only);
    assert_eq!(none.kimi(), PalwModelLifecycleV1::Candidate, "the window still needs an admitted attempt somewhere");

    // The own-attempt anchor of the span before seats on both sides: the rule as it was is the window's special case.
    let own = own_at(995);
    assert_eq!(to_audit(on(), &f, root, &operands, 995, &own).kimi(), PalwModelLifecycleV1::Prefetching);
    assert_eq!(to_audit(params(), &f, root, &operands, 995, &own).kimi(), PalwModelLifecycleV1::Prefetching);
}

#[test]
fn m2_an_anchor_recorded_long_before_the_audit_survives_the_span_boundaries_between() {
    let (operands, root) = inventory();
    let f = fold(kimi_work());
    // Span 80 (DAA 800) holds the only attempt; spans 98 (the proofs) and 100 (the audit) open after it and nothing else carries one.
    let early = own_at(800);
    let fenced = to_audit(on(), &f, root, &operands, 800, &early);
    let anchor = fenced.state.round_seed_anchor().expect("still the span-80 anchor");
    assert_eq!(anchor.span, 80, "span 98's and span 100's openings did not clear it");
    assert_eq!(fenced.kimi(), PalwModelLifecycleV1::Prefetching, "20 spans back: inside the window");
    let below = to_audit(params(), &f, root, &operands, 800, &early);
    assert!(below.state.round_seed_anchor().is_none(), "below the fence every opening clears it");
    assert_eq!(below.kimi(), PalwModelLifecycleV1::Candidate, "and the audit finds none");
}

#[test]
fn m3_the_window_reaches_24_spans_back_and_no_further() {
    let (operands, root) = inventory();
    let f = fold(kimi_work());
    // The audit is span 100. Span 76 is 24 back (read); span 75 is 25 back (not).
    let w = to_audit(on(), &f, root, &operands, 760, &own_at(760));
    assert_eq!(w.kimi(), PalwModelLifecycleV1::Prefetching, "an anchor of span 76: the oldest the window reads");
    let older = to_audit(on(), &f, root, &operands, 750, &own_at(750));
    assert_eq!(older.state.round_seed_anchor().map(|a| a.span), Some(75), "the anchor is still there");
    assert_eq!(older.kimi(), PalwModelLifecycleV1::Candidate, "but an anchor of span 75 is outside the window: no jury sits");
}

#[test]
fn m3_the_juries_population_is_cut_at_the_anchors_span_so_a_bond_registered_after_the_seed_is_not_on_it() {
    let (operands, root) = inventory();
    let f = fold(kimi_work());
    // The class and its registrant register at DAA 100; every operator that could sit on the jury registers at DAA 800.
    let network = contested_network(root, false);
    let split = register_class_and_bond().len() + 2;
    let (class_part, operator_part) = network.split_at(split);
    let proofs: Vec<PalwConsensusObjectV2> = SYBILS.chain(HONEST).map(|n| proof(&operands, bond_key(n), 98)).collect();
    let run = |anchor_daa: u64| {
        let mut walk = Walk::new(on(), &f);
        let c = walk.next_ctx(100);
        let (s1, _) = fold_step(&walk.state, &walk.p, &c, class_part, None, &armed(None)).unwrap();
        walk.keep(s1);
        // The anchor is recorded before the operators register (DAA 790, span 79) or after (DAA 810, span 81).
        if anchor_daa < 800 {
            walk.own_anchor(anchor_daa).block(800, operator_part);
        } else {
            walk.block(800, operator_part).own_anchor(anchor_daa);
        }
        walk.block(985, &proofs).block(1_000, &[]);
        walk
    };
    let before = run(790);
    assert_eq!(before.state.round_seed_anchor().map(|a| a.span), Some(79));
    assert_eq!(
        before.kimi(),
        PalwModelLifecycleV1::Candidate,
        "the cut is DAA 790: no operator was registered when the seed existed, so no jury can be drawn from them"
    );
    let after = run(810);
    assert_eq!(after.kimi(), PalwModelLifecycleV1::Prefetching, "the cut is DAA 810: the operators registered at 800 are the population");
}

#[test]
fn before_the_fence_the_jurys_chain_is_byte_identical_to_one_with_no_fence_at_all() {
    let (operands, root) = inventory();
    let f = fold(kimi_work());
    let own_late: Box<dyn Fn(&mut Walk<'_>)> = Box::new(own_at(995));
    let merged_late: Box<dyn Fn(&mut Walk<'_>)> = Box::new(merged_at(995, block(0x7E), h64(0xAE)));
    let own_early: Box<dyn Fn(&mut Walk<'_>)> = Box::new(own_at(800));
    for (name, at, anchor) in [("own at 995", 995, &own_late), ("merged at 995", 995, &merged_late), ("own at 800", 800, &own_early)] {
        let a = to_audit(far(), &f, root, &operands, at, &**anchor);
        let b = to_audit(params(), &f, root, &operands, at, &**anchor);
        assert_eq!(a.roots, b.roots, "{name}: the same state root at every block under a fence that is never reached");
        assert_eq!(a.kimi(), b.kimi(), "{name}");
    }
}
