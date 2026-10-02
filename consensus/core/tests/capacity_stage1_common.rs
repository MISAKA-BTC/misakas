//! **ADR-0160 stage 1 (rcore/cap-s1) — the shared fixture** of `palw_capacity_stage1_state_diff` and
//! `palw_capacity_stage1_invariants`. Included through `#[path]`; as its own test target it holds no
//! test.
//!
//! [`Sim`] is a [`Chain`] (testnet-12's own fold, `rcore_common`) driven with its own blue-score counter,
//! so several blocks may share a DAA, by any number of producer bonds; every block is checked as
//! `Chain::step_at` checks one (the delta re-applies and reverts, the carriage reloads under its root)
//! and recorded on a [`TapeBlock`] tape (parent, delta, child), so the invariants suite can revert,
//! re-apply, restart and re-fold it.
#![allow(dead_code, unused_imports)]

#[path = "rcore_common.rs"]
mod common;
pub use common::*;

pub use kaspa_consensus_core::config::params::{ForkActivation, PALW_T12_CAPACITY_FENCES_V1, palw_t12_release_v2_params};
pub use kaspa_consensus_core::palw_state_v2::{PalwBlockContextV2, PalwStateDeltaV2, PalwVoidReasonV2, palw_bond_committed_raw_v1};
pub use std::collections::BTreeMap;

/// 1 MSK in sompi.
pub const MSK: u64 = 100_000_000;
/// The capacity fences' height on an armed ruleset — above the release's 750 (their prerequisites)
/// and below every block a scenario folds (the chain starts at DAA 1,000).
pub const H: u64 = 1_001;
/// The court challenger of a conviction (L-T6's bond 61) and the DA accuser (bond 1).
pub const CHALLENGER: u64 = 61;
pub const ACCUSER: u64 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Class {
    Floor,
    K8,
    M2,
}

impl Class {
    pub fn label(self) -> &'static str {
        match self {
            Class::Floor => "floor",
            Class::K8 => "8k",
            Class::M2 => "2M",
        }
    }

    /// Attempt blocks in the state diff's fill: past the largest ceiling any twin reaches (156 floor or
    /// 8k claims at 1M by the 500‰ ceiling; the 2M row's C7 cap is one).
    pub fn fill_blocks(self) -> u64 {
        match self {
            Class::Floor | Class::K8 => 164,
            Class::M2 => 6,
        }
    }
}

/// **Stage 1's fences** — the capacity list's lanes the stage integrates (F-W, F-E, F-L at ρ = 1, F-B,
/// F-R); stage 2's F-Q and F-S (and every later stage's fence) stay dormant in a stage-1 twin.
pub const STAGE1_FENCES: [&str; 5] = [
    "palw_capacity_weight_cap",
    "palw_capacity_escrow_at_licence",
    "palw_capacity_aggregate_liability",
    "palw_capacity_batch_licence",
    "palw_capacity_verify_room",
];

/// **Stage 2's fences** — the audit door (F-Q) and the issuance slots (F-S).
pub const STAGE2_FENCES: [&str; 2] = ["palw_capacity_audit_door", "palw_capacity_issuance_slots"];

/// testnet-12 as shipped for `class` (the 2M row opened by its flag-day row, `t12_2m_flag_day_row`:
/// at launch it takes no claim at all), and with stage 1's capacity fences ([`STAGE1_FENCES`]) armed at
/// [`H`] where `armed`.
pub fn params_for(class: Class, armed: bool) -> Params {
    let mut p = palw_t12_release_v2_params();
    if class == Class::M2 {
        p.palw_class_verify_rows = Box::leak(Box::new([t12_2m_flag_day_row()]));
        p.sync_palw_class_verify_deadline();
        p.validate_palw_v2().expect("the 2M flag-day fixture validates on the shipped release");
    }
    if armed {
        for fence in PALW_T12_CAPACITY_FENCES_V1.iter().filter(|f| STAGE1_FENCES.contains(&f.name)) {
            (fence.set)(&mut p, Some(ForkActivation::new(H)));
        }
        p.validate_palw_v2().expect("stage 1's capacity fences over the release");
    }
    p
}

/// One recorded block: its inputs, its parent, its delta and the child it left.
#[derive(Clone)]
pub struct SimBlock {
    pub daa: u64,
    /// The block's blue score (the claims it accepts record it, so a replay must reuse it).
    pub blue: u64,
    pub objects: Vec<PalwConsensusObjectV2>,
    /// The attempt the block carried, `(bond n, seed)`, if the fold took it into the block (an attempt
    /// the fold refused at block level is not recorded: the block was folded without it).
    pub attempt: Option<(u64, u64)>,
    pub parent: PalwChainStateV2,
    pub delta: PalwStateDeltaV2,
    pub child: PalwChainStateV2,
}

/// A [`Chain`] folded with its own blue-score counter by any number of producer bonds.
pub struct Sim {
    pub c: Chain,
    pub blue: u64,
    pub model: Option<Hash64>,
    /// The fold's skip messages ([`reason_key`]), counted, and block-level refusals.
    pub skips: BTreeMap<String, usize>,
    /// Every folded block, in order (from [`Sim::new`]'s registration block on).
    pub tape: Vec<SimBlock>,
    /// The state before the registration block.
    pub base: PalwChainStateV2,
}

impl Sim {
    /// `class` made `Active` and its seats ready (for a model class), then `producers` (`(n,
    /// collateral MSK)`), the court's challenger and the DA accuser registered in one block — the same
    /// objects on every twin.
    pub fn new(p: Params, class: Class, producers: &[(u64, u64)]) -> Sim {
        let mut c = Chain::new(p);
        c.attribution = true;
        let model = match class {
            Class::Floor => None,
            Class::K8 => Some(model_classes(&c.p).0),
            Class::M2 => Some(model_classes(&c.p).1),
        };
        if let Some(class_id) = model {
            c.room = true;
            let honest = honest(&c.p);
            c.s = readied(&c.sp, &activated(&c.sp, &c.s, class_id), &honest, class_id, c.daa);
        }
        let blue = c.daa;
        let base = c.s.clone();
        let mut sim = Sim { c, blue, model, skips: BTreeMap::new(), tape: Vec::new(), base };
        let mut objects: Vec<PalwConsensusObjectV2> = producers.iter().map(|(n, msk)| bond_obj(*n, msk * MSK)).collect();
        objects.push(bond_obj(CHALLENGER, 400_000 * MSK));
        objects.push(bond_obj(ACCUSER, 50_000 * MSK));
        let daa = sim.c.daa + 1;
        sim.block(daa, objects, None);
        sim
    }

    /// Re-prove the genesis cards ready for the model class (testnet-12's readiness horizon).
    pub fn reready(&mut self) {
        if let Some(class_id) = self.model {
            let honest = honest(&self.c.p);
            self.c.s = readied(&self.c.sp, &self.c.s, &honest, class_id, self.c.daa);
        }
    }

    /// The attempt envelope bond `n` makes with `seed` at `daa`, its execution key, its claim id and the
    /// job anchor the fold is handed.
    fn attempt(
        &self,
        n: u64,
        seed: u64,
        daa: u64,
    ) -> (kaspa_consensus_core::palw_attempt_v2::PalwAttemptEnvelopeV2, Hash64, Hash64, Hash64) {
        match self.model {
            None => {
                let (env, key, id) = floor_attempt_of(&self.c, n, seed);
                (env, key, id, floor_job_anchor(&self.c.p, bond_key(n), 0x10C0 + seed))
            }
            Some(class_id) => {
                let pwu = class_pwu(&self.c.p, &self.c.s, class_id, daa);
                let (env, key, id) =
                    junk_attempt(class_id, bond_key(n), pubkey_of(n), &operator_pubkey_of(n), pwu, seed, 0x5_0000 + seed);
                (env, key, id, Hash64::default())
            }
        }
    }

    /// One block at `daa` (≥ the tip's) with `objects` and, optionally, bond `n`'s attempt `seed`
    /// (`Some((n, seed))`). Returns the attempt's claim id if the fold recorded it; a skip is counted
    /// and the block stands; an attempt the fold refuses at BLOCK level (the class gate: a producer's
    /// pre-check never mines it) is counted and the block is folded without it.
    pub fn block(&mut self, daa: u64, objects: Vec<PalwConsensusObjectV2>, attempt: Option<(u64, u64)>) -> Option<Hash64> {
        assert!(daa >= self.c.daa, "DAA never falls");
        self.reready();
        let asked = attempt;
        let attempt = attempt.map(|(n, seed)| self.attempt(n, seed, daa));
        self.blue += 1;
        let c = &mut self.c;
        let mut e = c.extras_at(daa);
        let (work, key) = match &attempt {
            Some((env, key, _, anchor)) => {
                e.own_job_anchor = *anchor;
                (PalwBlockWorkV3::Attempt(env), *key)
            }
            None => (PalwBlockWorkV3::None, Hash64::default()),
        };
        let subsidy = if attempt.is_some() { T12_BLOCK_SUBSIDY_SOMPI } else { 0 };
        let x = PalwBlockContextV2 { block: h(0xD1FF_0000_0000 + self.blue), daa_score: daa, blue_score: self.blue, subsidy };
        let parent = c.s.clone();
        let folded = fold_with(&c.p, &c.sp, &parent, &x, &objects, work, key, &e);
        let (child, delta, skips, attempt, carried) = match (folded, attempt) {
            (Ok((child, delta, skips)), attempt) => (child, delta, skips, attempt, asked),
            (Err(err), Some(_)) => {
                *self.skips.entry(format!("(block) {}", reason_key(&err.to_string()))).or_insert(0) += 1;
                let e = c.extras_at(daa);
                let (child, delta, skips) =
                    fold_with(&c.p, &c.sp, &parent, &x, &objects, PalwBlockWorkV3::None, Hash64::default(), &e)
                        .unwrap_or_else(|err| panic!("the block at DAA {daa} folds without the attempt: {err}"));
                (child, delta, skips, None, None)
            }
            (Err(err), None) => panic!("the block at DAA {daa} folds: {err}"),
        };
        assert_eq!(apply_delta_v2(&parent, &delta, &c.sp).expect("re-applies"), child, "DAA {daa}: the delta is the transition");
        assert_eq!(revert_delta_v2(&child, &delta, &c.sp).expect("reverts"), parent, "DAA {daa}: the delta reverts");
        let reloaded =
            PalwStateCarriageV2::from_state(&child).into_state(&c.sp, Some(child.state_root())).expect("the carriage reloads");
        assert_eq!(reloaded, child, "DAA {daa}: reload is the state");
        for (_, reason) in &skips {
            *self.skips.entry(reason_key(reason)).or_insert(0) += 1;
        }
        self.tape.push(SimBlock { daa, blue: self.blue, objects, attempt: carried, parent, delta, child: child.clone() });
        c.s = child;
        c.daa = daa;
        attempt.and_then(|(_, _, id, _)| c.s.claim(&id).is_some().then_some(id))
    }

    /// **One block at `daa` with `objects` and no attempt, if the fold takes it**: `Err` (the fold's
    /// refusal, the state untouched) where [`Sim::block`] would panic.
    pub fn try_block(&mut self, daa: u64, objects: Vec<PalwConsensusObjectV2>) -> Result<(), String> {
        self.reready();
        let x =
            PalwBlockContextV2 { block: h(0xD1FF_0000_0000 + self.blue + 1), daa_score: daa, blue_score: self.blue + 1, subsidy: 0 };
        let e = self.c.extras_at(daa);
        fold_with(&self.c.p, &self.c.sp, &self.c.s, &x, &objects, PalwBlockWorkV3::None, Hash64::default(), &e)
            .map_err(|err| err.to_string())?;
        self.block(daa, objects, None);
        Ok(())
    }

    /// A copy of this simulation standing on the same tip (the same rules, blue counter and model),
    /// with an empty tape — a trial branch.
    pub fn fork(&self) -> Sim {
        let c = Chain {
            p: self.c.p.clone(),
            sp: self.c.sp.clone(),
            s: self.c.s.clone(),
            daa: self.c.daa,
            room: self.c.room,
            attribution: self.c.attribution,
        };
        Sim { c, blue: self.blue, model: self.model, skips: BTreeMap::new(), tape: Vec::new(), base: self.c.s.clone() }
    }

    /// **Back to tip `j`** (the state after the tape's `j`-th block; 0 = [`Sim::base`]'s successor is
    /// not reachable, so `j ≥ 1`): the tape truncated there, the chain standing on that block's child.
    /// Returns the blocks taken off, oldest first.
    pub fn rewind_to(&mut self, j: usize) -> Vec<SimBlock> {
        assert!(j >= 1 && j <= self.tape.len(), "tip {j} of {}", self.tape.len());
        let off = self.tape.split_off(j);
        let tip = &self.tape[j - 1];
        self.c.s = tip.child.clone();
        self.c.daa = tip.daa;
        self.blue = tip.blue;
        off
    }

    /// Fold `blocks` again, input for input (the same DAA, blue score and block hash), on the current
    /// tip; each child must be the recorded one (root and state).
    pub fn replay(&mut self, blocks: &[SimBlock]) {
        for (i, b) in blocks.iter().enumerate() {
            self.blue = b.blue - 1;
            self.block(b.daa, b.objects.clone(), b.attempt);
            assert_eq!(self.c.s.state_root(), b.child.state_root(), "replayed block {i} (DAA {}): the recorded root", b.daa);
            assert_eq!(self.c.s, b.child, "replayed block {i} (DAA {}): the recorded state", b.daa);
        }
    }

    /// The next block, with `objects`.
    pub fn step(&mut self, objects: Vec<PalwConsensusObjectV2>) {
        let daa = self.c.daa + 1;
        self.block(daa, objects, None);
    }

    /// Bond `n`'s attempt `seed` in the next block; its claim id if recorded.
    pub fn claim(&mut self, n: u64, seed: u64) -> Option<Hash64> {
        let daa = self.c.daa + 1;
        self.block(daa, vec![], Some((n, seed)))
    }

    /// `claim` bound to `seats` in the next block; the bind's DAA.
    pub fn bind(&mut self, claim: Hash64, seats: &[(PalwBondKeyV2, Hash64)]) -> u64 {
        let anchor = h(0xAC_0000 + self.c.daa);
        self.step(vec![PalwConsensusObjectV2::PanelBound { claim, anchor, seats: seats_of(seats) }]);
        assert!(matches!(self.c.claim(&claim).phase, PalwClaimPhaseV2::PanelBound { .. }), "the panel binds");
        self.c.daa
    }

    /// `claim` licensed by a `Valid` from each of `seats` in the next block.
    pub fn license(&mut self, claim: Hash64, seats: &[(PalwBondKeyV2, Hash64)], bound: u64) {
        self.step(vec![PalwConsensusObjectV2::ReceiptLicensed {
            claim,
            receipts: seats.iter().map(|(k, _)| valid(claim, *k, bound)).collect(),
        }]);
        assert!(matches!(self.c.claim(&claim).phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "the licence folds");
    }

    /// One block past the latest `Final` deadline of `claims` (each licensed).
    pub fn finalize_all(&mut self, claims: &[Hash64]) {
        let at = claims.iter().filter_map(|id| self.c.s.deadline_of(id)).max().map_or(self.c.daa + 1, |d| d + 1).max(self.c.daa + 1);
        self.block(at, vec![], None);
        for id in claims {
            assert!(matches!(self.c.claim(id).phase, PalwClaimPhaseV2::Final { .. }), "{id} Final at {at}");
        }
    }

    /// A proven court verdict on licensed `claim` (the intent class): opened by the challenger,
    /// closed `ExecutorGuilty` — two blocks.
    pub fn court_fraud(&mut self, claim: Hash64) {
        let open = court_opened(&self.c.s, claim, bond_key(CHALLENGER));
        self.step(vec![open]);
        let session = court_session_of(&self.c.s, claim, bond_key(CHALLENGER));
        self.step(vec![guilty_close(session)]);
        assert!(
            matches!(self.c.claim(&claim).phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtFraud, .. }),
            "the verdict voids {claim} CourtFraud"
        );
    }

    /// The seats a panel of this class binds.
    pub fn seats(&self) -> Vec<(PalwBondKeyV2, Hash64)> {
        panel_seats(&self.c, self.model)
    }
}

/// The seats a panel of `class` binds: the floor's five genesis seats, or five ready genesis cards.
pub fn panel_seats(c: &Chain, model: Option<Hash64>) -> Vec<(PalwBondKeyV2, Hash64)> {
    match model {
        None => c.floor_seats(),
        Some(_) => honest_seats(&c.p, 5),
    }
}

/// `guilty_close` of the liab suite: a court's `ExecutorGuilty` close on an arithmetic proof (the fold
/// reads the verdict, never the proof).
pub fn guilty_close(session_id: Hash64) -> PalwConsensusObjectV2 {
    let PalwConsensusObjectV2::CourtClosed { proof, .. } = court_cleared(session_id) else { unreachable!("a close") };
    PalwConsensusObjectV2::CourtClosed {
        session_id,
        verdict: kaspa_consensus_core::palw_state_v2::PalwCourtVerdictV2::ExecutorGuilty,
        proof,
    }
}

/// A skip reason with every bond key elided (`PalwBondKeyV2(..)` → `<bond>`), first 110 characters.
pub fn reason_key(reason: &str) -> String {
    let mut out = String::new();
    let mut rest = reason;
    while let Some(i) = rest.find("PalwBondKeyV2(") {
        out.push_str(&rest[..i]);
        out.push_str("<bond>");
        let tail = &rest[i + "PalwBondKeyV2(".len()..];
        let (mut depth, mut end) = (1usize, tail.len());
        for (j, ch) in tail.char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = j + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
        rest = &tail[end..];
    }
    out.push_str(rest);
    out.chars().take(110).collect()
}
