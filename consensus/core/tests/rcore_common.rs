//! **ADR-0152 R-core+ — the shared fixture of the `rcore_s*` suites.** Included through `#[path]`;
//! as its own test target it holds no test.
//!
//! [`Chain`] folds one block per step through testnet-12's own fold with the extras the processor
//! resolves (`dos_l5_common`'s with the execution lane, or `panel_room_common`'s for the model
//! classes' gate — [`Chain::extras_at`]) and checks every block three ways: the delta
//! re-applies to the child and reverts to the parent, and the child's carriage reloads under its
//! committed root (`into_state`: the ledger re-derived from the claims' commitments, R-core+'s load
//! invariants, DL-1's deadlines exactly).
#![allow(dead_code, unused_imports)]

#[path = "panel_room_common.rs"]
mod room;
pub use room::*;

pub use kaspa_consensus_core::Hash64;
pub use kaspa_consensus_core::config::params::Params;
pub use kaspa_consensus_core::palw_economic_safety_v1::PalwLicenceDoorTagV1;
pub use kaspa_consensus_core::palw_panel_v2::{PalwReceiptVerdictV2, PalwSeatReceiptV2, PalwSeatReceiptV3};
pub use kaspa_consensus_core::palw_pwu::palw_pwu_v1;
pub use kaspa_consensus_core::palw_state_v2::{
    PalwBlockWorkV3, PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwClaimRcoreV1, PalwClaimStateV2, PalwConsensusObjectV2,
    PalwStateCarriageV2, PalwStateParamsV2, PalwTransitionExtrasV1, apply_delta_v2, palw_claim_commitment_v1,
    palw_rcore_counts_licensed_v1, revert_delta_v2,
};
pub use kaspa_consensus_core::palw_verification_v2::palw_segment_assignment_v2;

/// testnet-12 with `palw_rcore_plus = None` (C7 cleared, mirrors re-synced) — the fence-off twin.
pub fn twin(p: &Params) -> Params {
    let mut t = p.clone();
    t.palw_rcore_plus = None;
    t.palw_rcore_conservative_classes = &[];
    t.sync_palw_rcore_plus();
    t
}

/// The genesis bond that produces the floor claims, with its keys (`dos_l5_4b`'s executor).
pub fn floor_producer(p: &Params) -> (PalwBondKeyV2, Vec<u8>, Vec<u8>) {
    for o in bundle(p).genesis_objects.iter() {
        if let PalwConsensusObjectV2::BondRegistered { bond, pubkey, operator_pubkey, .. } = o {
            return (*bond, pubkey.clone(), operator_pubkey.clone());
        }
    }
    unreachable!("testnet-12 registers genesis bonds")
}

/// A chain on testnet-12 (or its twin): one block per step through the real fold, each checked.
pub struct Chain {
    pub p: Params,
    pub sp: PalwStateParamsV2,
    pub s: PalwChainStateV2,
    pub daa: u64,
    /// Whether steps fold with `panel_room`'s extras (the model classes' gate) or `dos_l5`'s.
    pub room: bool,
    /// Whether steps fold with `palw_offence_attribution` armed as the processor resolves it on
    /// testnet-12 (`dos_l5`'s extras hold it dormant, deliberately, for the V1-route suites): the
    /// court's defaults are then `CourtDefault` and kinds 3 and 4 are admitted.
    pub attribution: bool,
}

impl Chain {
    pub fn new(p: Params) -> Self {
        let sp = bundle(&p).state.clone();
        let s = genesis_state(&p);
        Self { p, sp, s, daa: 1_000, room: false, attribution: false }
    }

    /// One block at `daa`: folded, re-applied, reverted, reloaded.
    pub fn step_at(&mut self, daa: u64, objects: &[PalwConsensusObjectV2], work: PalwBlockWorkV3<'_>, key: Hash64, subsidy: u64) {
        assert!(daa > self.daa || self.daa == 1_000, "DAA moves forward");
        self.daa = daa;
        let c = ctx(0xCA_0000 + daa, daa, daa, subsidy);
        let parent = self.s.clone();
        let (child, delta, skips) =
            self.try_fold(&parent, &c, objects, work, key).unwrap_or_else(|e| panic!("the block at DAA {daa} folds: {e}"));
        assert!(skips.is_empty(), "nothing skipped at {daa}: {skips:?}");
        assert_eq!(apply_delta_v2(&parent, &delta, &self.sp).expect("re-applies"), child, "DAA {daa}: the delta is the transition");
        assert_eq!(revert_delta_v2(&child, &delta, &self.sp).expect("reverts"), parent, "DAA {daa}: the delta reverts");
        let reloaded = PalwStateCarriageV2::from_state(&child)
            .into_state(&self.sp, Some(child.state_root()))
            .unwrap_or_else(|e| panic!("DAA {daa}: the carriage reloads (ledger, R-core+ invariants, DL-1): {e}"));
        assert_eq!(reloaded, child, "DAA {daa}: reload is the state");
        self.s = child;
    }

    /// **The extras this chain folds with at `daa`** — the processor's: `dos_l5`'s with the execution
    /// lane as the processor resolves it ([`processor_round_lane`]: its quantum counts a claim's
    /// execution rights `R`, the S review's L1), or `panel_room`'s (the model classes' gate) on a
    /// model chain.
    pub fn extras_at(&self, daa: u64) -> PalwTransitionExtrasV1 {
        let mut e = if self.room {
            room_extras(&self.p, daa)
        } else {
            let mut e = extras(&self.p, daa);
            e.round_lane = processor_round_lane(&self.p, daa);
            e
        };
        e.offence_attribution_active = self.attribution && self.p.palw_offence_attribution_active_at(daa);
        e
    }

    /// One block on `parent` with this chain's extras ([`Self::extras_at`]), unchecked — the probe a
    /// test takes beside [`Self::step_at`].
    pub fn try_fold(
        &self,
        parent: &PalwChainStateV2,
        c: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
        objects: &[PalwConsensusObjectV2],
        work: PalwBlockWorkV3<'_>,
        key: Hash64,
    ) -> Result<
        (PalwChainStateV2, kaspa_consensus_core::palw_state_v2::PalwStateDeltaV2, Vec<(Hash64, String)>),
        kaspa_consensus_core::palw_state_v2::PalwStateV2Error,
    > {
        fold_with(&self.p, &self.sp, parent, c, objects, work, key, &self.extras_at(c.daa_score))
    }

    pub fn step(&mut self, objects: &[PalwConsensusObjectV2]) {
        self.step_at(self.daa + 1, objects, PalwBlockWorkV3::None, Hash64::default(), 0);
    }

    pub fn claim(&self, id: &Hash64) -> PalwClaimStateV2 {
        self.s.claim(id).expect("the claim is live").clone()
    }

    pub fn reserved(&self, bond: &PalwBondKeyV2) -> u128 {
        self.s.reserved_exposure(bond)
    }

    /// **The one pwu admission accepts for a floor attempt at `daa`** (ADR-0149): past the
    /// canonical-work height `palw_attempt_derived_pwu_v1(effective target, the floor's derived draw)`,
    /// the draw from the floor's row or, before it has one, from the registry's table
    /// (`base_known_draw`); below it the declared rule. What the processor's pre-check and the
    /// producer's facts hand a producer, so the claim's weight term is the real one (ADR §2's `w`).
    pub fn floor_pwu(&self, daa: u64) -> u64 {
        let (floor, leaves, target, _) = genesis_classes(&self.p)[0];
        let Some(height) = self.p.palw_canonical_work_daa().filter(|height| daa >= *height) else {
            return palw_pwu_v1(target, leaves);
        };
        let base_known = registry_fold(&self.p, daa).and_then(|fold| fold.genesis_works.get(&floor).map(|w| w.economic_ccu_per_claim));
        let per_draw =
            self.s.palw_attempt_per_draw_v1(&floor, &floor, daa, Some(height), base_known).expect("the floor's draw is derivable");
        let effective = kaspa_consensus_core::palw_admission_v2::palw_effective_class_target_v1(&self.s, &self.sp, &floor, None)
            .expect("the floor's effective target");
        kaspa_consensus_core::palw_admission_v2::palw_attempt_derived_pwu_v1(effective, per_draw)
    }

    /// A floor attempt by the first genesis bond, accepted in its own block.
    pub fn floor_claim(&mut self, seed: u64) -> Hash64 {
        let (floor, _, _, _) = genesis_classes(&self.p)[0];
        let (bond, pubkey, operator) = floor_producer(&self.p);
        let pwu = self.floor_pwu(self.daa + 1);
        let (env, key, id) = junk_attempt(floor, bond, pubkey, &operator, pwu, seed, 0x10C0 + seed);
        self.step_at(self.daa + 1, &[], PalwBlockWorkV3::Attempt(&env), key, T12_BLOCK_SUBSIDY_SOMPI);
        assert!(self.s.claim(&id).is_some(), "the floor attempt is accepted");
        id
    }

    /// The floor claims' panel: the five genesis bonds after the producer.
    pub fn floor_seats(&self) -> Vec<(PalwBondKeyV2, Hash64)> {
        genesis_bonds(&self.p)[1..6].iter().map(|(k, o, _)| (*k, *o)).collect()
    }

    pub fn bind(&mut self, claim: Hash64, seats: &[(PalwBondKeyV2, Hash64)]) -> u64 {
        self.step(&[PalwConsensusObjectV2::PanelBound { claim, anchor: h(0xAC_0000 + self.daa), seats: seats_of(seats) }]);
        assert!(matches!(self.claim(&claim).phase, PalwClaimPhaseV2::PanelBound { .. }), "the panel binds");
        self.daa
    }

    /// Finalize by stepping past the challenge window.
    pub fn finalize(&mut self, claim: Hash64) {
        let PalwClaimPhaseV2::ReceiptLicensed { licensed_daa } = self.claim(&claim).phase else { panic!("licensed first") };
        let at = licensed_daa + self.sp.window_challenge_at(licensed_daa) + 1;
        self.step_at(at, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
        assert!(matches!(self.claim(&claim).phase, PalwClaimPhaseV2::Final { .. }), "Final at {at}");
    }

    pub fn anchor(&self, claim: &Hash64) -> Hash64 {
        self.s.panel(claim).expect("a bound panel").anchor
    }
}

pub fn receipt(claim: Hash64, seat: PalwBondKeyV2, verdict: PalwReceiptVerdictV2, signed_daa: u64) -> PalwSeatReceiptV2 {
    PalwSeatReceiptV2 { claim, verdict, seat_bond: seat, signed_daa, signature: Vec::new() }
}

pub fn valid(claim: Hash64, seat: PalwBondKeyV2, signed_daa: u64) -> PalwSeatReceiptV2 {
    receipt(claim, seat, PalwReceiptVerdictV2::Valid, signed_daa)
}

pub fn unavailable(claim: Hash64, seat: PalwBondKeyV2, signed_daa: u64) -> PalwSeatReceiptV2 {
    receipt(claim, seat, PalwReceiptVerdictV2::Unavailable { chunk_index: 0, requested_daa: signed_daa }, signed_daa)
}

/// **The `G_res` a licence in the next block records** (the S-4 review's G freeze): the live
/// residual gain `palw_rcore_bind_prices_v1` reads — the value `rcore_backed_set` prices the licence
/// set's locks with — on the current (pre-licence) state, at the next DAA and its extras.
pub fn licence_g_res(c: &Chain, id: &Hash64) -> u128 {
    let at = c.daa + 1;
    let claim = c.claim(id);
    let seats = c.s.panel(id).map(|panel| panel.seats.len()).unwrap_or(5);
    kaspa_consensus_core::palw_state_v2::palw_rcore_bind_prices_v1(&c.s, &c.sp, &c.extras_at(at), id, &claim, seats, at).g_res
}

/// The escrow term `E` of `claim` (option A's reservation of its escrow), as the ledger holds it.
pub fn escrow(sp: &PalwStateParamsV2, claim: &PalwClaimStateV2) -> u128 {
    sp.claim_escrow_reservation_v1(claim.accepted_daa, claim.escrowed_reward)
}

/// A model-class chain: `class` made `Active`, the genesis bonds proved ready, and `producers` rich
/// bonds registered — `panel_room`'s fixture.
pub fn model_chain(p: Params, class: Hash64, producers: u64) -> Chain {
    let mut c = Chain::new(p);
    c.room = true;
    let honest = honest(&c.p);
    c.s = readied(&c.sp, &activated(&c.sp, &c.s, class), &honest, class, c.daa);
    let bonds: Vec<_> = (1..=producers).map(|n| bond_obj(n, RICH)).collect();
    c.step(&bonds);
    c
}

/// A model-class attempt by rich bond `n`, accepted in its own block (the room re-readied first).
pub fn model_claim(c: &mut Chain, class: Hash64, n: u64, seed: u64) -> Hash64 {
    c.s = readied(&c.sp, &c.s, &honest(&c.p), class, c.daa);
    let pwu = class_pwu(&c.p, &c.s, class, c.daa + 1);
    let (env, key, id) = junk_attempt(class, bond_key(n), pubkey_of(n), &operator_pubkey_of(n), pwu, seed, 0x5_0000 + seed);
    c.step_at(c.daa + 1, &[], PalwBlockWorkV3::Attempt(&env), key, T12_BLOCK_SUBSIDY_SOMPI);
    assert!(c.s.claim(&id).is_some(), "the model attempt is accepted");
    id
}

/// The coverage door's receipts: each seat's `Valid` with its assigned mask (the full seat's full).
pub fn covered(
    claim: Hash64,
    anchor: Hash64,
    seats: &[(PalwBondKeyV2, Hash64)],
    take: &[usize],
    signed: u64,
) -> Vec<PalwSeatReceiptV3> {
    let a = palw_segment_assignment_v2(anchor, claim, seats.len() as u16);
    take.iter().map(|i| PalwSeatReceiptV3 { receipt: valid(claim, seats[*i].0, signed), segments: a.mask_of(*i as u16) }).collect()
}

// ---------------------------------------------------------------------------------------------
// ADR-0152 M5 (§7.1, §8.3 item 1): fold / reorg / restart on the merged result.
//
// A [`Tape`] is a [`Chain`] whose every committed block is recorded — its inputs, the delta it
// wrote and the state it left — so a test can revert the run block by block to any earlier tip and
// re-apply it (a reorg), fold it again from its base (an IBD), and decode any tip's carriage from
// its bytes under its committed root and fold on from there (a restart), each compared with the
// uninterrupted run. A tape takes no carriage edits: every state on it is a fold's.
// ---------------------------------------------------------------------------------------------

/// A floor attempt by bond `n` with the floor's registered artifact root and its own execution key
/// (the shape `rcore_s3`, `rcore_m3` and `rcore_one_invariant` each build locally).
pub fn floor_attempt_of(
    c: &Chain,
    n: u64,
    seed: u64,
) -> (kaspa_consensus_core::palw_attempt_v2::PalwAttemptEnvelopeV2, Hash64, Hash64) {
    let (floor, _, _, _) = genesis_classes(&c.p)[0];
    let pwu = c.floor_pwu(c.daa + 1);
    let (mut env, _, _) = junk_attempt(floor, bond_key(n), pubkey_of(n), &operator_pubkey_of(n), pwu, seed, 0x10C0 + seed);
    env.attempt.artifact_root = c.s.class(&floor).expect("the floor").artifact_root;
    let anchor = kaspa_consensus_core::palw_attempt_v2::execution_anchor_v3(h(NET), h(0x10C0 + seed), floor, &bond_key(n).0, 7);
    let key = kaspa_consensus_core::palw_attempt_v2::execution_commitment_v3(&env.attempt, anchor);
    let id = kaspa_consensus_core::palw_attempt_v2::attempt_id_v2(&env.attempt);
    (env, key, id)
}

/// An event accusation of `(row, 0)` (the fold never reads the signature: the acceptance layer's).
pub fn da_accuse(claim: Hash64, accuser: PalwBondKeyV2, row: u32) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::DefaultAccused {
        claim,
        missing_event_index: kaspa_consensus_core::palw_state_v2::palw_da_event_index_v1(row, 0),
        accuser,
        signature: vec![],
    }
}

/// A standalone `ExecutorEquivocation` (kind 0) against `bond` on `class`'s job, its two
/// attestations' roots drawn from `nonce` (a distinct nonce is a distinct `evidence_id`), and the
/// evidence id the fold consumes it under — `rcore_s4`'s shape with a nonce. The acceptance layer
/// verified the certificate; the fold reads the class and the accused bond from it.
pub fn equivocation_of(bond: PalwBondKeyV2, class: Hash64, nonce: u64) -> (PalwConsensusObjectV2, Hash64) {
    use kaspa_consensus_core::palw_offence_v1::{PalwOffenceKindV1, palw_offence_evidence_digest_v1};
    let floor =
        kaspa_consensus_core::palw_base0_profile::base0_profile_v1(kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_GEOMETRY)
            .expect("the floor profile");
    let mut job_context = kaspa_consensus_core::palw_base0_profile::rc_job_context(&floor, 512, 256);
    job_context.shape_profile_id = class;
    let attestation = |root: u64| kaspa_consensus_core::palw_slash::PalwExecutionAttestationV1 {
        version: kaspa_consensus_core::palw_slash::PALW_S_OBJECT_VERSION_V3,
        executor_id: h(0x1),
        job_context_hash: h(0x2),
        full_logits_trace_root: h(root),
        committed_root: h(root),
        bond_outpoint: bond.0,
        signature: Vec::new(),
    };
    let carriage = kaspa_consensus_core::palw_carriage::PalwEquivocationCarriageV1 {
        version: kaspa_consensus_core::palw_carriage::PALW_CARRIAGE_VERSION_V1,
        accused_bond_outpoint: bond.0,
        certificate: kaspa_consensus_core::palw_slash::PalwClassContradictionCertificateV1 {
            version: kaspa_consensus_core::palw_slash::PALW_S_OBJECT_VERSION_V3,
            job_context,
            attestation_a: attestation(0xAA00_0000 ^ nonce),
            attestation_b: attestation(0xBB00_0000 ^ nonce),
        },
    };
    let evidence = borsh::to_vec(&carriage).unwrap();
    let evidence_id = palw_offence_evidence_digest_v1(&evidence);
    (
        PalwConsensusObjectV2::ObjectiveOffence {
            kind: PalwOffenceKindV1::ExecutorEquivocation,
            accused: bond,
            evidence_id,
            evidence,
        },
        evidence_id,
    )
}

/// The ledger key an `ExecutorEquivocation` against `bond` with `evidence_id` is consumed under.
pub fn equivocation_key(bond: PalwBondKeyV2, evidence_id: Hash64) -> Hash64 {
    kaspa_consensus_core::palw_offence_v1::palw_offence_id_v1(
        kaspa_consensus_core::palw_offence_v1::PalwOffenceKindV1::ExecutorEquivocation,
        &bond.0,
        &evidence_id,
    )
}

/// Reporter `reporter`'s salt in these tests.
pub fn reporter_salt(reporter: PalwBondKeyV2) -> [u8; 32] {
    let mut salt = [0x5A; 32];
    for (i, b) in borsh::to_vec(&reporter).expect("a bond key encodes").into_iter().enumerate() {
        salt[i % 32] ^= b;
    }
    salt
}

/// `reporter`'s commitment (R-3, N12) to `(offence_key, evidence_id)` under [`reporter_salt`], as
/// object 53 (the fold never reads the signature: the acceptance layer's).
pub fn reporter_commit(offence_key: Hash64, evidence_id: Hash64, reporter: PalwBondKeyV2) -> PalwConsensusObjectV2 {
    let commitment = kaspa_consensus_core::palw_state_v2::palw_reporter_commitment_v1(
        &offence_key,
        &evidence_id,
        &reporter,
        &reporter_salt(reporter),
    );
    PalwConsensusObjectV2::ReporterCommitted { commitment, reporter, signature: vec![1] }
}

/// `reporter`'s reveal (object 54) of its commitment to `offence_key`.
pub fn reporter_reveal(offence_key: Hash64, reporter: PalwBondKeyV2) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::ReporterRevealed { offence_key, reporter, salt: reporter_salt(reporter) }
}

/// The attempt a recorded block carries: the envelope, its execution key, and the carrying header's
/// execution anchor the key was derived under (the processor's `extras.own_job_anchor`).
pub type TapeAttempt = (kaspa_consensus_core::palw_attempt_v2::PalwAttemptEnvelopeV2, Hash64, Hash64);

/// The execution anchor [`junk_attempt`] and [`floor_attempt_of`] derive a floor attempt's key under
/// for bond `bond` and pre-PoW word `pre_pow` (nonce 7).
pub fn floor_job_anchor(p: &Params, bond: PalwBondKeyV2, pre_pow: u64) -> Hash64 {
    let (floor, _, _, _) = genesis_classes(p)[0];
    kaspa_consensus_core::palw_attempt_v2::execution_anchor_v3(h(NET), h(pre_pow), floor, &bond.0, 7)
}

/// One recorded block: its inputs, the delta it wrote, the state it left and what it skipped.
#[derive(Clone)]
pub struct TapeBlock {
    pub daa: u64,
    pub objects: Vec<PalwConsensusObjectV2>,
    pub attempt: Option<TapeAttempt>,
    pub subsidy: u64,
    /// The carrying header's execution anchor the processor hands the fold for the block's own
    /// attempt (`extras.own_job_anchor`, recorded as the claim's `job_identity` past
    /// `palw_offence_attribution`); zero for a block without one.
    pub job_anchor: Hash64,
    pub delta: kaspa_consensus_core::palw_state_v2::PalwStateDeltaV2,
    pub state: PalwChainStateV2,
    pub skips: usize,
}

/// A [`Chain`] whose every committed block is recorded (see the section note above).
pub struct Tape {
    pub c: Chain,
    pub base: PalwChainStateV2,
    pub base_daa: u64,
    pub blocks: Vec<TapeBlock>,
    /// Whether blocks fold with the free-prompt lane's two extras as the processor resolves them on
    /// testnet-12 (`fp_derived_work_daa`, `fp_da_pins_active`; `dos_repro_2`'s `t12_extras`) — the
    /// abandon-hold restart's tape ([`fp_floor_ready`]).
    pub fp_lane: bool,
}

/// The extras a tape folds a block with: the chain's ([`Chain::extras_at`]), the block's own
/// execution anchor, and the free-prompt lane's two where `fp_lane`.
pub fn tape_extras(chain: &Chain, fp_lane: bool, daa: u64, job_anchor: Hash64) -> PalwTransitionExtrasV1 {
    let mut e = chain.extras_at(daa);
    e.own_job_anchor = job_anchor;
    if fp_lane {
        e.fp_derived_work_daa = chain.p.palw_fp_derived_work_fence().map(|f| f.daa_score());
        e.fp_da_pins_active = chain.p.palw_fp_da_pins_fence().is_some_and(|f| f.is_active(daa));
    }
    e
}

impl Tape {
    pub fn new(c: Chain) -> Self {
        let (base, base_daa) = (c.s.clone(), c.daa);
        Self { c, base, base_daa, blocks: Vec::new(), fp_lane: false }
    }

    /// A chain with this tape's rules (params, extras flags) standing on `s` at `daa`.
    pub fn chain_on(&self, s: PalwChainStateV2, daa: u64) -> Chain {
        Chain { p: self.c.p.clone(), sp: self.c.sp.clone(), s, daa, room: self.c.room, attribution: self.c.attribution }
    }

    /// A new tape forked off this one at tip `j` (0 = the base): the same rules, standing on that tip.
    pub fn fork(&self, j: usize) -> Tape {
        let mut fork = Tape::new(self.chain_on(self.state_at(j).clone(), self.daa_at(j)));
        fork.fp_lane = self.fp_lane;
        fork
    }

    /// The state at tip `j` (0 = the base, `blocks.len()` = the tip).
    pub fn state_at(&self, j: usize) -> &PalwChainStateV2 {
        if j == 0 { &self.base } else { &self.blocks[j - 1].state }
    }

    pub fn daa_at(&self, j: usize) -> u64 {
        if j == 0 { self.base_daa } else { self.blocks[j - 1].daa }
    }

    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    #[allow(clippy::too_many_arguments)]
    fn fold_on(
        chain: &Chain,
        fp_lane: bool,
        parent: &PalwChainStateV2,
        daa: u64,
        objects: &[PalwConsensusObjectV2],
        attempt: Option<&TapeAttempt>,
        subsidy: u64,
        job_anchor: Hash64,
    ) -> Result<
        (PalwChainStateV2, kaspa_consensus_core::palw_state_v2::PalwStateDeltaV2, Vec<(Hash64, String)>),
        kaspa_consensus_core::palw_state_v2::PalwStateV2Error,
    > {
        let x = ctx(0xCA_0000 + daa, daa, daa, subsidy);
        let (work, key) = match attempt {
            Some((env, key, _)) => (PalwBlockWorkV3::Attempt(env), *key),
            None => (PalwBlockWorkV3::None, Hash64::default()),
        };
        fold_with(&chain.p, &chain.sp, parent, &x, objects, work, key, &tape_extras(chain, fp_lane, daa, job_anchor))
    }

    /// The block at `daa` folded on the tip, not committed.
    pub fn probe(
        &self,
        daa: u64,
        objects: &[PalwConsensusObjectV2],
        attempt: Option<&TapeAttempt>,
        subsidy: u64,
    ) -> Result<(PalwChainStateV2, Vec<(Hash64, String)>), kaspa_consensus_core::palw_state_v2::PalwStateV2Error> {
        Self::fold_on(&self.c, self.fp_lane, &self.c.s, daa, objects, attempt, subsidy, attempt.map(|a| a.2).unwrap_or_default())
            .map(|(child, _, skips)| (child, skips))
    }

    /// **One block at `daa`, committed and recorded** — checked as [`Chain::step_at`] checks one: the
    /// delta re-applies to the child and reverts to the parent, and the child's carriage reloads under
    /// its committed root. A fold error leaves the tape unchanged; a skipped own attempt is recorded.
    pub fn block(
        &mut self,
        daa: u64,
        objects: Vec<PalwConsensusObjectV2>,
        attempt: Option<TapeAttempt>,
        subsidy: u64,
    ) -> Result<Vec<(Hash64, String)>, kaspa_consensus_core::palw_state_v2::PalwStateV2Error> {
        assert!(daa > self.c.daa, "DAA moves forward: {daa} after {}", self.c.daa);
        let parent = self.c.s.clone();
        let job_anchor = attempt.as_ref().map(|a| a.2).unwrap_or_default();
        let (child, delta, skips) =
            Self::fold_on(&self.c, self.fp_lane, &parent, daa, &objects, attempt.as_ref(), subsidy, job_anchor)?;
        assert_eq!(apply_delta_v2(&parent, &delta, &self.c.sp).expect("re-applies"), child, "DAA {daa}: the delta is the transition");
        assert_eq!(revert_delta_v2(&child, &delta, &self.c.sp).expect("reverts"), parent, "DAA {daa}: the delta reverts");
        let reloaded = PalwStateCarriageV2::from_state(&child)
            .into_state(&self.c.sp, Some(child.state_root()))
            .unwrap_or_else(|e| panic!("DAA {daa}: the carriage reloads (ledger, R-core+ invariants, DL-1): {e}"));
        assert_eq!(reloaded, child, "DAA {daa}: reload is the state");
        self.c.s = child.clone();
        self.c.daa = daa;
        self.blocks.push(TapeBlock { daa, objects, attempt, subsidy, job_anchor, delta, state: child, skips: skips.len() });
        Ok(skips)
    }

    /// [`Self::block`] that must fold with nothing skipped.
    pub fn at(&mut self, daa: u64, objects: Vec<PalwConsensusObjectV2>) {
        let skips = self.block(daa, objects, None, 0).unwrap_or_else(|e| panic!("the block at DAA {daa} folds: {e}"));
        assert!(skips.is_empty(), "nothing skipped at {daa}: {skips:?}");
    }

    /// The next block (`daa + 1`).
    pub fn step(&mut self, objects: Vec<PalwConsensusObjectV2>) {
        self.at(self.c.daa + 1, objects);
    }

    /// A floor attempt by bond `n` (the genesis producer when `n` is `None`), accepted in its own block.
    pub fn attempt(&mut self, n: Option<u64>, seed: u64) -> Hash64 {
        let (floor, _, _, _) = genesis_classes(&self.c.p)[0];
        let (env, key, id, bond) = match n {
            Some(n) => {
                let (env, key, id) = floor_attempt_of(&self.c, n, seed);
                (env, key, id, bond_key(n))
            }
            None => {
                let (bond, pubkey, operator) = floor_producer(&self.c.p);
                let (env, key, id) =
                    junk_attempt(floor, bond, pubkey, &operator, self.c.floor_pwu(self.c.daa + 1), seed, 0x10C0 + seed);
                (env, key, id, bond)
            }
        };
        let anchor = floor_job_anchor(&self.c.p, bond, 0x10C0 + seed);
        let daa = self.c.daa + 1;
        let skips = self.block(daa, vec![], Some((env, key, anchor)), T12_BLOCK_SUBSIDY_SOMPI).expect("the attempt's block folds");
        assert!(skips.is_empty() && self.c.s.claim(&id).is_some(), "the attempt is accepted: {skips:?}");
        id
    }

    /// `claim` bound to the floor seats in the next block; returns the bind DAA.
    pub fn bind(&mut self, claim: Hash64) -> u64 {
        let seats = self.c.floor_seats();
        self.step(vec![PalwConsensusObjectV2::PanelBound { claim, anchor: h(0xAC_0000 + self.c.daa), seats: seats_of(&seats) }]);
        self.c.daa
    }

    /// A state's carriage encoded, decoded and loaded under its committed root exactly as the store
    /// loads it (`into_state_v3` with the processor's uncertified-weight rule at `daa` and its
    /// canonical-work height): the ledger, R-core+'s load invariants and DL-1's deadlines re-derived.
    pub fn load(
        &self,
        s: &PalwChainStateV2,
        daa: u64,
    ) -> Result<PalwChainStateV2, kaspa_consensus_core::palw_state_v2::PalwStateV2Error> {
        let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(s)).expect("the carriage encodes");
        let carriage: PalwStateCarriageV2 = borsh::from_slice(&bytes).expect("the carriage decodes");
        carriage.into_state_v3(
            &self.c.sp,
            Some(s.state_root()),
            flags(&self.c.p, daa).uncertified_weightless,
            self.c.p.palw_canonical_work_daa(),
        )
    }

    /// **The reorg to every earlier tip.** From the tip, each block's delta is reverted in turn and the
    /// parent it leaves must be the recorded state at that tip (equal, and the same root) — so every
    /// tip on the way down is a reorg target reached — and must LOAD from its carriage under its root
    /// (T40's load-time re-derivation after a revert: no `CarriageInconsistent`, the loaded state the
    /// reverted one); down to the base; then every block is applied again, back to the recorded tip.
    pub fn revert_to_base_and_reapply(&self) {
        let sp = &self.c.sp;
        let mut s = self.c.s.clone();
        for j in (0..self.blocks.len()).rev() {
            s = revert_delta_v2(&s, &self.blocks[j].delta, sp)
                .unwrap_or_else(|e| panic!("revert of block {j} (DAA {}): {e}", self.blocks[j].daa));
            let want = self.state_at(j);
            assert_eq!(s.state_root(), want.state_root(), "reverted to tip {j}: the root is the recorded one");
            assert_eq!(&s, want, "reverted to tip {j}: the state is the recorded one");
            let loaded = self.load(&s, self.daa_at(j)).unwrap_or_else(|e| panic!("reverted to tip {j}: the carriage loads: {e}"));
            assert_eq!(loaded, s, "reverted to tip {j}: the load re-derives the reverted state");
        }
        for (j, b) in self.blocks.iter().enumerate() {
            s = apply_delta_v2(&s, &b.delta, sp).unwrap_or_else(|e| panic!("re-apply of block {j}: {e}"));
            assert_eq!(s.state_root(), b.state.state_root(), "re-applied to tip {}: the root", j + 1);
            assert_eq!(s, b.state, "re-applied to tip {}", j + 1);
        }
    }

    /// **A reorg from this tape's tip to `other`'s** (a fork of this tape at tip `j`): this tape's
    /// blocks after `j` reverted, `other`'s applied; then back. Every state on the way is a recorded one.
    pub fn reorg_to(&self, j: usize, other: &Tape) {
        let sp = &self.c.sp;
        assert_eq!(other.base.state_root(), self.state_at(j).state_root(), "the fork stands on tip {j}");
        let mut s = self.c.s.clone();
        for i in (j..self.blocks.len()).rev() {
            s = revert_delta_v2(&s, &self.blocks[i].delta, sp).expect("the old branch reverts");
        }
        assert_eq!(&s, self.state_at(j), "the old branch reverted to the fork point");
        for b in other.blocks.iter() {
            s = apply_delta_v2(&s, &b.delta, sp).expect("the new branch applies");
            assert_eq!(s, b.state, "the new branch's recorded state");
        }
        for b in other.blocks.iter().rev() {
            s = revert_delta_v2(&s, &b.delta, sp).expect("the new branch reverts");
        }
        assert_eq!(&s, self.state_at(j), "and back to the fork point");
        for b in self.blocks[j..].iter() {
            s = apply_delta_v2(&s, &b.delta, sp).expect("the old branch re-applies");
        }
        assert_eq!(s, self.c.s, "and the old tip again");
    }

    /// **The IBD twin.** Every recorded block folded again, in order, from `base` (the tape's own base,
    /// or a state rebuilt from scratch that must equal it) by a fresh chain with this tape's rules: each
    /// child, delta and root equal to the recorded run's.
    pub fn ibd_from(&self, base: PalwChainStateV2) {
        assert_eq!(base.state_root(), self.base.state_root(), "the IBD starts from the tape's base");
        let chain = self.chain_on(base, self.base_daa);
        let mut s = chain.s.clone();
        for (j, b) in self.blocks.iter().enumerate() {
            let (child, delta, skips) =
                Self::fold_on(&chain, self.fp_lane, &s, b.daa, &b.objects, b.attempt.as_ref(), b.subsidy, b.job_anchor)
                    .unwrap_or_else(|e| panic!("IBD block {j} (DAA {}): {e}", b.daa));
            assert_eq!(skips.len(), b.skips, "IBD block {j}: the same skips");
            assert_eq!(delta, b.delta, "IBD block {j} (DAA {}): the same delta", b.daa);
            assert_eq!(child.state_root(), b.state.state_root(), "IBD block {j}: the same root");
            assert_eq!(child, b.state, "IBD block {j}: the same state");
            s = child;
        }
    }

    /// **The restart twin at tip `j`.** The tip's carriage is encoded, decoded and loaded under its
    /// committed root exactly as the store loads it (`into_state_v3` with the processor's
    /// uncertified-weight rule and canonical-work height) — no `CarriageInconsistent` — and equals the
    /// state it was written from; then every later recorded block is folded on the LOADED state, and
    /// each child, delta and root equals the uninterrupted run's.
    pub fn restart_at(&self, j: usize) -> PalwChainStateV2 {
        let at = self.state_at(j);
        let daa = self.daa_at(j);
        let loaded =
            self.load(at, daa).unwrap_or_else(|e| panic!("restart at tip {j} (DAA {daa}): the carriage loads under its root: {e}"));
        assert_eq!(&loaded, at, "restart at tip {j}: the loaded state is the one written");
        let chain = self.chain_on(loaded.clone(), daa);
        let mut s = loaded.clone();
        for (i, b) in self.blocks[j..].iter().enumerate() {
            let (child, delta, skips) =
                Self::fold_on(&chain, self.fp_lane, &s, b.daa, &b.objects, b.attempt.as_ref(), b.subsidy, b.job_anchor)
                    .unwrap_or_else(|e| panic!("restart at tip {j}: block {} (DAA {}): {e}", j + i, b.daa));
            assert_eq!(skips.len(), b.skips, "restart at tip {j}: block {}: the same skips", j + i);
            assert_eq!(delta, b.delta, "restart at tip {j}: block {} (DAA {}): the same delta", j + i, b.daa);
            assert_eq!(child, b.state, "restart at tip {j}: block {}: the same state as the uninterrupted run", j + i);
            s = child;
        }
        loaded
    }
}

/// A real-length ML-DSA-87 key for bond `n` (the fold matches a free-prompt commitment's executor key
/// against its bond's, `BondKeyMismatch`).
pub fn fp_pubkey_of(n: u64) -> Vec<u8> {
    let mut v = vec![0xA7u8; kaspa_consensus_core::mldsa87_primitives::MLDSA87_PUBKEY_LEN];
    v[..8].copy_from_slice(&n.to_le_bytes());
    v
}

/// **The floor's free-prompt lane made ready on `c`, and bond `n` registered to use it** (the setup
/// `dos_repro_2` documents): the BASE-0 FreePrompt family and the floor's registry row written through
/// the carriage — a live `FamilyCertified` drill and the lane's span step need inputs a core test
/// cannot supply, and both rows hold what the real fold would write — then the floor's FP work
/// profile published by a real `ClassLaneCertified` block, every setup block folded with the
/// processor's FP extras ([`tape_extras`]). Returns a tape standing on the ready state (its base,
/// `fp_lane` set) and the work leaves of the minimal one-token job.
pub fn fp_floor_ready(mut c: Chain, n: u64, collateral: u64) -> (Tape, u64) {
    use kaspa_consensus_core::palw_model_registry_v1::{
        PALW_REGISTRY_GLOBALS_V1, PalwModelLifecycleRowV1, PalwModelLifecycleV1, palw_lifecycle_profile_v1,
    };
    let fp_block = |c: &mut Chain, objects: &[PalwConsensusObjectV2]| {
        let daa = c.daa + 1;
        let x = ctx(0xCA_0000 + daa, daa, daa, 0);
        let e = tape_extras(c, true, daa, Hash64::default());
        let (child, _, skips) = fold_with(&c.p, &c.sp, &c.s, &x, objects, PalwBlockWorkV3::None, Hash64::default(), &e)
            .unwrap_or_else(|e| panic!("the FP setup block at DAA {daa} folds: {e}"));
        assert!(skips.is_empty(), "nothing skipped in the FP setup: {skips:?}");
        c.s = child;
        c.daa = daa;
    };
    fp_block(
        &mut c,
        &[PalwConsensusObjectV2::BondRegistered {
            bond: bond_key(n),
            pubkey: fp_pubkey_of(n),
            operator_pubkey: operator_pubkey_of(n),
            collateral,
            payout_payload: h(0x9A00 + n),
            capable_classes: Default::default(),
            signature: Vec::new(),
        }],
    );
    let (floor, _, target, _) = genesis_classes(&c.p)[0];
    let profile = Box::new(
        kaspa_consensus_core::palw_base0_profile::base0_profile_v1(kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_GEOMETRY)
            .expect("the floor profile"),
    );
    let family = kaspa_consensus_core::palw_e2e_adjudicability::palw_rc_fp_certified_families_v1()
        .into_iter()
        .find(|f| f.drilled_class_id == profile.shape_profile_id())
        .expect("the BASE-0 FreePrompt family is pinned by this build");
    let (pf, dc) = kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_CANONICAL;
    let job = kaspa_consensus_core::palw_base0_profile::rc_job_context(&profile, pf, dc);
    let work =
        kaspa_consensus_core::palw_model_registry_v1::palw_model_work_from_carriage_v1(&profile, &job).expect("the floor's work");
    let mut globals = PALW_REGISTRY_GLOBALS_V1;
    globals.seat_count = bundle(&c.p).panel.seat_count();
    let expected_q32 = kaspa_consensus_core::palw_economic_compute_v1::palw_expected_attempts_q32_v1(target);
    let row = PalwModelLifecycleRowV1 {
        state: PalwModelLifecycleV1::Active,
        work,
        profile: palw_lifecycle_profile_v1(&work, expected_q32, &globals, false),
        since_span: 0,
        probes_passed: 0,
        probes_failed: 0,
        probes_passed_this_span: 0,
        probes_failed_this_span: 0,
        ready_seats: 0,
        inflight_claims: 0,
        utilization_permille: 0,
        admission_milli: 0,
        cap_utilization_permille: 0,
        priced_share_permille: 0,
    };
    let mut carriage = PalwStateCarriageV2::from_state(&c.s);
    carriage
        .fp_certified_families
        .insert(family.digest(), kaspa_consensus_core::palw_state_v2::PalwCertifiedFamilyStateV2 { family, certified_daa: 0 });
    carriage.model_lifecycles.insert(floor, row);
    c.s = carriage.into_state_v3(&c.sp, None, false, c.p.palw_canonical_work_daa()).expect("the carriage rebuilds");
    fp_block(
        &mut c,
        &[PalwConsensusObjectV2::ClassLaneCertified {
            class_id: floor,
            lane: kaspa_consensus_core::palw_state_v2::PalwCertifiedLaneV1::FreePrompt,
            profile: profile.clone(),
        }],
    );
    assert!(c.s.fp_work_profile_of(&floor).is_some(), "the floor's FP work profile is rooted");
    let ladder = c.s.class_step_ladder_v1(&floor, kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_STRUCTURAL_WORK_LEAVES_CAP);
    let leaves = kaspa_consensus_core::palw_step::step_leaf_count_of_tokens_capped_v1(&profile, 1, 1, ladder)
        .expect("the minimal job's leaves");
    let mut t = Tape::new(c);
    t.fp_lane = true;
    (t, leaves)
}

/// A minimal free-prompt commitment by bond `n` on the floor (a distinct one-token prompt, one decode
/// token), and its claim id: `dos_repro_2`'s job.
pub fn fp_commit_of(c: &Chain, n: u64, work_leaves: u64, i: u64) -> (PalwConsensusObjectV2, Hash64) {
    let (floor, _, _, _) = genesis_classes(&c.p)[0];
    let ids = vec![i as u32];
    let claim = h(0xF0_0000_0000 + i);
    (
        PalwConsensusObjectV2::FreePromptCommitted {
            job_pin: Hash64::default(),
            claim,
            class_id: floor,
            bond: bond_key(n),
            executor_pubkey: fp_pubkey_of(n),
            work_leaves,
            prompt_token_ids_hash: kaspa_consensus_core::palw_v2::prompt_token_ids_hash_v2(&ids),
            prompt_tokens: 1,
            prompt_token_ids: ids,
            decode_tokens_executed: 1,
            trace_root: h(0x71_0000_0000 + i),
            output_root: h(0x72_0000_0000 + i),
            execution_root: h(0x73_0000_0000 + i),
            trace_chunk_count: 1,
            trace_retention_daa: 9_999_999,
            consumed_prefix_state: kaspa_consensus_core::palw_freeprompt_v3::PalwFpPrefixStateV1::genesis(floor),
        },
        claim,
    )
}
