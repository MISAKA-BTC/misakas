//! **The OPV test world** (RFC-0015): the ledger-test world of `ledger_world`, with an OPV policy at genesis, the class registered
//! under `OptimisticPublicVerification` (route tag 13) and extra bonds for squatters and an honest second producer. A Panel-licensed
//! class of the same program can be registered beside it (`register_panel_class`) to show the two modes side by side.
//!
//! The policy values below are **examples for tests**, chosen for no network.
#![allow(dead_code)]

use super::chain::{Consumer, T};
use super::ledger_world::*;
use super::{active_for, bump, root_of};
use misaka_palw_kernel::descriptor::{k2_tir_v1_descriptor, k2_tir_v2_descriptor};
use misaka_palw_kernel::hash::Digest;
use misaka_palw_kernel::job::KernelJobV1;
use misaka_palw_kernel::ledger::{
    AuthV1, KernelLedgerV1, KernelRouteObjectV1 as O, LedgerBlockV1, LedgerEventV1 as E, LedgerPolicyV1, LedgerTxV1,
    single_class_id_v1,
};
use misaka_palw_kernel::mode::VerificationModeV1;
use misaka_palw_kernel::opv::{CarrierCapsV1, OpvBudgetsV1, OpvEconomicsV1, OpvPolicyV1, OpvWindowV1};
use misaka_palw_kernel::trace::ParamCommitmentsV1;
use misaka_palw_tir::program::StateKind;
use misaka_palw_tir_sketch::fixture::dense_moe_v1;

/// A second producer, an honest one that wants the job a squatter holds.
pub const HONEST: Digest = [0x4E; 64];
/// A squatter: a bond that commits junk claims.
pub const SQUATTER: Digest = [0x5C; 64];

/// An example OPV policy: a 50-DAA window (40 base + 10 horizon), budgets that fit it, a 1000 reservation per claim.
pub fn opv_example() -> OpvPolicyV1 {
    OpvPolicyV1 {
        activation_daa: Some(0),
        window: OpvWindowV1 { base_challenge_window_daa: 40, verification_horizon_daa: 10 },
        budgets: OpvBudgetsV1 {
            cold_material_daa: 10,
            check_daa: 10,
            localize_daa: 2,
            disclose_daa: 8,
            court_daa: 3,
            carrier_daa: 2,
            reorg_slack_daa: 2,
        },
        economics: OpvEconomicsV1 {
            reservation_per_claim: 1000,
            work_credit_per_claim: 13,
            external_gain_bound: 80,
            assumed_detection_permille: 500,
            max_live_claims_per_producer: 3,
            max_live_claims_total: 5,
            default_burn_permille: 100,
            admission_fee: 3,
        },
        carrier: CarrierCapsV1 { filing_cap: 1 << 26, response_cap: 1 << 27, commit_cap: 1 << 27 },
    }
}

/// The genesis ledger of a network with the OPV policy (and no class yet).
pub fn opv_genesis(policy: LedgerPolicyV1, opv: OpvPolicyV1) -> KernelLedgerV1 {
    let d = k2_tir_v1_descriptor();
    KernelLedgerV1::genesis(policy, active_for(&d), vec![k2_tir_v1_descriptor(), k2_tir_v2_descriptor()])
        .unwrap()
        .with_opv_policy(opv)
        .unwrap()
}

/// The ledger inputs that attest the artifact, have the network's policy admit the class (for a non-legacy mode) and register the
/// world's program under `mode` (route tag 13), signed by `signer`. The registration object is the LAST input.
pub fn register_in(w: &World, mode: VerificationModeV1, signer: Digest) -> Vec<LedgerTxV1> {
    let T::RegisterClass { descriptor, program_bytes, plan, params } = w.register() else { unreachable!() };
    let pc = ParamCommitmentsV1::of(&params);
    let mut txs = vec![LedgerTxV1::AttestArtifact { artifact_root: pc.root() }];
    if mode != VerificationModeV1::PanelLicensed {
        txs.push(LedgerTxV1::AdmitOptimisticClass { class: single_class_id_v1(descriptor, &program_bytes, &plan, &pc, mode) });
    }
    txs.push(LedgerTxV1::Object {
        auth: AuthV1 { signer_bond: signer },
        object: O::RegisterClassV2 { mode, descriptor, program_bytes, plan, param_commitments: pc },
    });
    txs
}

impl World {
    pub fn new_opv() -> Self {
        Self::with_opv(policy(), opv_example())
    }

    /// A world whose genesis carries `opv`, with the class registered under `OptimisticPublicVerification` in block 1.
    pub fn with_opv(policy: LedgerPolicyV1, opv: OpvPolicyV1) -> Self {
        let fx = dense_moe_v1(7);
        // The fixture declares the held-history envelope of a long context (2^18 rows): its worst opening is ~32 MB and a position's
        // worst response ~190 MB, which no route object can carry — an OPV class must fit its carriers (RFC-0015 §6.3), so this
        // world's class declares a 64-row history (the plan's `MAX_POSITIONS`). The values and relations are the fixture's own.
        let mut program = fx.program;
        for st in program.states.iter_mut() {
            if let StateKind::Hist { window } = &mut st.kind {
                *window = 64;
            }
        }
        let genesis = opv_genesis(policy, opv);
        let mut w = World {
            genesis: genesis.clone(),
            blocks: vec![],
            l: genesis,
            consumer: Consumer::default(),
            events: vec![],
            class: [0; 64],
            program,
            params: fx.params,
        };
        let mut txs: Vec<LedgerTxV1> = Vec::new();
        for (b, c) in [(PRODUCER, 5000), (OUTSIDER, 1000), (SPAM1, 1000), (SPAM2, 1000), (HONEST, 3000), (SQUATTER, 3000)] {
            txs.push(LedgerTxV1::SyncBond { bond: b, collateral: c });
        }
        txs.extend(register_in(&w, VerificationModeV1::OptimisticPublicVerification, PRODUCER));
        let ev = w.block_raw(1, txs);
        if let Some(class) = ev.iter().find_map(|e| if let E::ClassRegistered { class } = e { Some(*class) } else { None }) {
            w.class = class;
        }
        w
    }

    /// Apply a block of raw ledger inputs through the consumer (the same path `block` takes).
    pub fn block_raw(&mut self, daa: u64, txs: Vec<LedgerTxV1>) -> Vec<E> {
        let b = LedgerBlockV1 { daa, txs };
        let ev = self.consumer.apply(&mut self.l, &b);
        self.blocks.push(b);
        self.events.extend(ev.iter().cloned());
        self.l.opv_invariants().unwrap_or_else(|why| panic!("OPV invariant broken at {daa}: {why}"));
        ev
    }

    /// Register the same program under the legacy Panel-licensed mode (route tag 1) and return its class id.
    pub fn register_panel_class(&mut self, daa: u64) -> Digest {
        let ev = self.block(daa, vec![self.register()]);
        ev.iter()
            .find_map(|e| if let E::ClassRegistered { class } = e { Some(*class) } else { None })
            .unwrap_or_else(|| panic!("the Panel-licensed class did not register: {ev:?}"))
    }

    /// Run `f` with the world's class temporarily set to `class` (to post jobs and produce claims for another mode's class).
    pub fn with_class<R>(&mut self, class: Digest, f: impl FnOnce(&mut World) -> R) -> R {
        let saved = std::mem::replace(&mut self.class, class);
        let r = f(self);
        self.class = saved;
        r
    }

    pub fn root_of_program(&self) -> Digest {
        root_of(&self.program)
    }

    /// An honest claim of `job` by `bond`.
    pub fn honest_by(&self, job: &KernelJobV1, bond: Digest, n: usize) -> Produced {
        let generated = self.greedy(&self.params, &job.prompt, n);
        self.produce(job, bond, generated, &self.params, |_| {})
    }

    /// A claim of `job` by `bond` whose committed trace lies at one MatMul (position 1).
    pub fn lying_by(&self, job: &KernelJobV1, bond: Digest, n: usize) -> ((u32, u16, u16), Produced) {
        let at = self.matmul_at(1);
        let generated = self.greedy(&self.params, &job.prompt, n);
        let p =
            self.produce(job, bond, generated, &self.params, |t| bump(&mut t.values[at.0 as usize][at.1 as usize][at.2 as usize], 1));
        (at, p)
    }

    /// A world whose ledger carries `opv` but whose only class is the legacy Panel-licensed one (block 1 registers it by tag 1).
    pub fn panel_world_with_opv(policy: LedgerPolicyV1, opv: OpvPolicyV1) -> Self {
        let fx = dense_moe_v1(7);
        let genesis = opv_genesis(policy, opv);
        let mut w = World {
            genesis: genesis.clone(),
            blocks: vec![],
            l: genesis,
            consumer: Consumer::default(),
            events: vec![],
            class: [0; 64],
            program: fx.program,
            params: fx.params,
        };
        let ev = w.block(
            1,
            vec![
                super::ledger_world::bond(PRODUCER, 5000),
                super::ledger_world::bond(OUTSIDER, 1000),
                super::ledger_world::bond(SPAM1, 1000),
                super::ledger_world::bond(SPAM2, 1000),
                w.register(),
            ],
        );
        w.class = ev.iter().find_map(|e| if let E::ClassRegistered { class } = e { Some(*class) } else { None }).expect("registers");
        w
    }
}
