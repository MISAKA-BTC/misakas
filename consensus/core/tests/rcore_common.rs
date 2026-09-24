//! **ADR-0152 R-core+ — the shared fixture of the `rcore_s*` suites.** Included through `#[path]`;
//! as its own test target it holds no test.
//!
//! [`Chain`] folds one block per step through testnet-12's own fold (`dos_l5_common`'s extras, or
//! `panel_room_common`'s for the model classes' gate) and checks every block three ways: the delta
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
    PalwStateCarriageV2, PalwStateParamsV2, apply_delta_v2, palw_claim_commitment_v1, palw_rcore_counts_licensed_v1, revert_delta_v2,
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
}

impl Chain {
    pub fn new(p: Params) -> Self {
        let sp = bundle(&p).state.clone();
        let s = genesis_state(&p);
        Self { p, sp, s, daa: 1_000, room: false }
    }

    /// One block at `daa`: folded, re-applied, reverted, reloaded.
    pub fn step_at(&mut self, daa: u64, objects: &[PalwConsensusObjectV2], work: PalwBlockWorkV3<'_>, key: Hash64, subsidy: u64) {
        assert!(daa > self.daa || self.daa == 1_000, "DAA moves forward");
        self.daa = daa;
        let c = ctx(0xCA_0000 + daa, daa, daa, subsidy);
        let parent = self.s.clone();
        let (child, delta, skips) = if self.room {
            go(&self.p, &self.sp, &parent, &c, objects, work, key)
        } else {
            fold(&self.p, &self.sp, &parent, &c, objects, work, key)
        }
        .unwrap_or_else(|e| panic!("the block at DAA {daa} folds: {e}"));
        assert!(skips.is_empty(), "nothing skipped at {daa}: {skips:?}");
        assert_eq!(apply_delta_v2(&parent, &delta, &self.sp).expect("re-applies"), child, "DAA {daa}: the delta is the transition");
        assert_eq!(revert_delta_v2(&child, &delta, &self.sp).expect("reverts"), parent, "DAA {daa}: the delta reverts");
        let reloaded = PalwStateCarriageV2::from_state(&child)
            .into_state(&self.sp, Some(child.state_root()))
            .unwrap_or_else(|e| panic!("DAA {daa}: the carriage reloads (ledger, R-core+ invariants, DL-1): {e}"));
        assert_eq!(reloaded, child, "DAA {daa}: reload is the state");
        self.s = child;
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
