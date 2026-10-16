//! **Shared owning39 fold fixture** (RFC-0002 native v39): the isolated test genesis, the processor-mirrored kernel-route
//! extras, a checked one-block-per-step chain, the fixture source/job builders and the production audit/anchor/redemption
//! plumbing. Included by `rfc0002_native39_lifecycle.rs` and by the SDK receipt-window test through `#[path]`; it holds no test.
#![allow(dead_code, unused_imports)]

#[path = "../dos_l5_common.rs"]
pub mod common;
pub use common::*;

pub use kaspa_consensus_core::Hash64;
pub use kaspa_consensus_core::config::params::{ForkActivation, Params};
pub use kaspa_consensus_core::palw_kernel_route_v1::{PalwKernelOpvExtrasV1, PalwKernelRouteExtrasV1, PalwRewardGateTermsV1};
pub use kaspa_consensus_core::palw_native_calibration_v34::{LocalNativePurposeV34, local_validation_snapshot_v34};
pub use kaspa_consensus_core::palw_native_calibration_v39::{NativeCalibrationSnapshotV39, load_isolated_native_calibration_v39};
pub use kaspa_consensus_core::palw_native_execution_v39::{NativeClassWireV39, NativeExecutionPoliciesV39};
pub use kaspa_consensus_core::palw_native_final_v27::NativeFinalCalibrationV27;
pub use kaspa_consensus_core::palw_native_job_v39::{
    CheckedNativeMaximumV39, NativeCommitWireV39, NativeConsentV39, NativeConsentWireV39, NativeFundWireV39,
};
pub use kaspa_consensus_core::palw_state_v2::{
    PalwBlockWorkV3, PalwBondKeyV2, PalwChainStateV2, PalwConsensusObjectV2, PalwStateCarriageV2, PalwStateDeltaV2,
    PalwStateParamsV2, PalwStateV2Error, apply_delta_v2, apply_palw_transition_v7, revert_delta_v2,
};
pub use misaka_palw_kernel::flat_job39::{ActualInputResolverV39, CheckedJobMaximumV39, JobMaximumV39, MaterialManifestV39, RequestStopV39};
pub use misaka_palw_kernel::flat_material39::{MaterialPageReaderV39, MaterialPageV39};
pub use misaka_palw_kernel::flat_quota11::owning39::derive_quota_v39;
pub use misaka_palw_kernel::flat_source39::{CheckedSourceV39, FlatSourceWireV39, derive_inventory_root_v39};
pub use misaka_palw_kernel::flat_trace39::{IgnoreTraceV39, trace_streamed_v39};
pub use misaka_palw_kernel::hash::Digest;
pub use misaka_palw_kernel::route::KernelRouteObjectV1;
pub use misaka_palw_kernel::trace::ParamCommitmentsV1;
pub use misaka_palw_tir::interp_v39::{ParamDescriptorV39, ParamSourceV39};
pub use misaka_palw_tir::program_v2::OutputDecl;
pub use misaka_palw_tir::program_v39::*;
pub use misaka_palw_tir::{ConstDecl, DType, Prim, Ref, Schedule, TirError, TirErrorKind, TirResult};

// ---------------------------------------------------------------------------------------------------------------------------
// The isolated test genesis
// ---------------------------------------------------------------------------------------------------------------------------

/// FIXTURE input of the isolated genesis's Final environment for the ordinary court profile — NOT a measurement. The path sum
/// (1+1+1+1+0 = 4 DAA) fits the probe policy's 4-DAA challenge window, the only constraint the fold places on it; the byte/work
/// ceilings are wide so the tiny fixture source is always covered. The roots are distinct labelled stand-ins.
pub fn fixture_calibration() -> NativeFinalCalibrationV27 {
    NativeFinalCalibrationV27 {
        profile: misaka_palw_kernel::flat_resources39::ordinary_court_id_v39(),
        maximum_reference_bytes: 1 << 40,
        maximum_court_bytes: 1 << 40,
        maximum_court_work: 1 << 50,
        maximum_court_scratch: 1 << 40,
        // Final27's well-formedness requires a positive fraction; v39 reports it as a metric only (never a Final gate).
        whole_claim_detection_numerator: 1,
        whole_claim_detection_denominator: 2,
        measured_cold_reference_daa: 1,
        measured_sequential_discovery_daa: 1,
        measured_prove_before_seal_daa: 1,
        measured_proof_delivery_daa: 1,
        restart_reorg_margin_daa: 0,
        measurement_root: [0xC1; 64],
        source_availability_root: [0xC2; 64],
        whole_claim_detection_root: [0xC4; 64],
        real_delivery_service_root: [0xC3; 64],
        independent_native_final_root: [0; 64],
        original_source_ready: true,
        whole_claim_detector_ready: true,
        actual_delivery_ready: true,
        independent_node_ready: false,
    }
}

/// **The isolated genesis's funded horizons** (a TEST-GENESIS policy, not a measurement). `local_validation_snapshot_v34` copies the
/// unapproved probe's horizons (`da_answer` 3 DAA, quota interval 1,000), and with the reserved scheduler's 2-DAA slot period no
/// native39 Fund fits: every complete paged carriage reserves at least 88 answer packets (11 x 2 x 4) inside min(answer, commit)
/// window = 3 DAA, ~60x the shared physical service (`admit_paged_v29`; pinned by
/// `rfc0002_native39_probe_horizons_admit_no_fund`). Here the answer and commit windows are 400 DAA and the interval 40,000 (two
/// sequential Master components fit one interval), so the same carriage fits (~0.46 of the service); every other
/// funded/quota/discovery value is the probe value.
pub const TEST_DA_ANSWER: u64 = 400;
pub const TEST_INTERVAL: u64 = 40_000;
pub const TEST_HORIZON: u64 = 16_000;

pub fn snapshot34(base: &Params, wide: bool) -> kaspa_consensus_core::palw_native_calibration_v34::NativeCalibrationSnapshotV34 {
    let mut snapshot = local_validation_snapshot_v34(
        base,
        LocalNativePurposeV34::Measurement,
        [0x39; 64],
        [0x3A; 64],
        [0x3B; 64],
        vec![fixture_calibration()],
        false,
        false,
    )
    .expect("the isolated calibration34 snapshot assembles on the devnet base");
    if wide {
        let f = &mut snapshot.funded;
        f.quota.interval_daa = TEST_INTERVAL;
        f.execution.horizons.da_answer = TEST_DA_ANSWER;
        f.execution.horizons.commit_window = TEST_DA_ANSWER;
        f.execution.quota_policy_root = f.quota.root();
        f.discovery.quota_policy_root = f.quota.root();
        snapshot.apply(base, snapshot.root()).expect("the widened isolated snapshot still validates");
    }
    snapshot
}

/// The isolated native39 ruleset: devnet base → calibration34 (`Measurement`) → owning39 wrap → the shared loader. Returns the
/// loaded `Params` (which passed `validate_palw_v2` inside `apply`).
pub fn native39_params_with(wide: bool) -> Params {
    let base = kaspa_consensus_core::config::params::devnet_shipped_params();
    let snapshot = snapshot34(&base, wide);
    let raw34 = borsh::to_vec(&snapshot).unwrap();
    let horizon = if wide { TEST_HORIZON } else { 900 };
    let wrapped = NativeCalibrationSnapshotV39::wrap(&base, raw34, snapshot.root(), horizon, 8).expect("owning39 wraps calibration34");
    let raw39 = borsh::to_vec(&wrapped).unwrap();
    let p = load_isolated_native_calibration_v39(&base, &raw39, wrapped.root()).expect("the shared loader accepts the pinned snapshot");
    p.validate_palw_v2().expect("the isolated native39 ruleset validates");
    p
}
pub fn native39_params() -> Params {
    native39_params_with(true)
}

pub fn network_domain(p: &Params) -> Hash64 {
    kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(p.net.to_string().as_bytes(), Some(p.genesis.hash))
}

pub fn active<T>(f: Option<&T>, act: impl Fn(&T) -> ForkActivation, daa: u64) -> bool {
    f.is_some_and(|f| act(f) != ForkActivation::never() && act(f).is_active(daa))
}

/// `palw_kernel_route_extras_at` rebuilt from `Params` (see the module doc).
pub fn kernel_route_extras(p: &Params, daa: u64) -> PalwKernelRouteExtrasV1 {
    let opv_at = active(p.palw_panel_free_v1.as_ref(), |f| f.activation, daa);
    let funded = active(p.palw_funded_execution_v11.as_ref(), |f| f.activation, daa) && opv_at;
    let final27 = active(p.palw_native_final_v27.as_ref(), |f| f.activation, daa) && opv_at;
    let mass = funded.then(|| kaspa_consensus_core::palw_native_carriage_v22::NativeCarriageMassParamsV22::from_consensus_params(p));
    let native_final = final27.then(|| p.palw_native_final_v27.clone().unwrap());
    let service = mass.as_ref().map(|mass| {
        native_final
            .as_ref()
            .and_then(|f| {
                kaspa_consensus_core::palw_flat_execution_capacity_v22::NativeCarriageServiceV22::reserved_scheduler_v30(
                    mass,
                    &f.policy.queue_policy,
                )
                .ok()
            })
            .unwrap_or_else(|| kaspa_consensus_core::palw_flat_execution_capacity_v22::NativeCarriageServiceV22::probe(mass))
    });
    let lanes39 = active(p.palw_native_execution_v39.as_ref(), |f| f.activation, daa) && final27;
    let mut ruleset = [0u8; 64];
    ruleset[..32].copy_from_slice(&p.consensus_identity_id().as_bytes());
    let fence = p.palw_panel_free_v1.as_ref().filter(|f| f.activation != ForkActivation::never());
    PalwKernelRouteExtrasV1 {
        network_domain: network_domain(p),
        ruleset_digest: Hash64::from_bytes(ruleset),
        chain_genesis: p.genesis.hash,
        native_carriage_mass: mass,
        native_carriage_service: service,
        native_final,
        native_execution39: lanes39.then(|| p.palw_native_execution_v39.clone().unwrap()),
        native_mining39: p
            .palw_native_mining_v39
            .as_ref()
            .filter(|f| f.activation != ForkActivation::never() && f.activation.is_active(daa) && lanes39)
            .cloned(),
        funded_execution: funded.then(|| p.palw_funded_execution_v11.clone().unwrap()),
        opv: fence.map(|fence| PalwKernelOpvExtrasV1 {
            policy: fence.opv_policy(),
            denied_classes: fence.denied_classes.clone(),
            min_effective_bits: fence.min_effective_bits,
            sampled_conformance_gates_reward: fence.sampled_conformance_gates_reward,
            test_eligible: Vec::new(),
            reward_gate: fence
                .activation
                .is_active(daa)
                .then(|| PalwRewardGateTermsV1 { fence_activation_daa: fence.activation.daa_score() }),
            budget_required: true,
        }),
        window_class_v37: p.palw_window_class_v37.filter(|f| *f != ForkActivation::never() && f.is_active(daa)).map(|f| f.daa_score()),
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------------------------------------------------------
// The chain: one block per step through the real fold, each checked
// ---------------------------------------------------------------------------------------------------------------------------

pub struct N39 {
    pub p: Params,
    pub sp: PalwStateParamsV2,
    pub s: PalwChainStateV2,
    pub daa: u64,
}

impl N39 {
    pub fn new() -> Self {
        Self::on(native39_params())
    }
    pub fn on(p: Params) -> Self {
        let sp = bundle(&p).state.clone();
        let s = genesis_state(&p);
        Self { p, sp, s, daa: 1_000 }
    }
    pub fn extras(&self, daa: u64) -> kaspa_consensus_core::palw_state_v2::PalwTransitionExtrasV1 {
        let mut e = extras(&self.p, daa);
        if self.p.palw_probabilistic_constraints_v1.is_some_and(|f| f.is_active(daa)) {
            e.kernel_route = Some(kernel_route_extras(&self.p, daa));
        }
        e
    }
    #[allow(clippy::type_complexity)]
    pub fn try_fold(
        &self,
        daa: u64,
        objects: &[PalwConsensusObjectV2],
    ) -> Result<(PalwChainStateV2, PalwStateDeltaV2, Vec<(Hash64, String)>), PalwStateV2Error> {
        let f = flags(&self.p, daa);
        apply_palw_transition_v7(
            &self.s,
            &self.sp,
            None,
            &ctx(0xCA_0000 + daa, daa, daa, 0),
            objects,
            PalwBlockWorkV3::None,
            &[],
            Hash64::default(),
            f.unavailable_abstains,
            f.capability_bound,
            f.uncertified_weightless,
            f.da_court,
            &self.extras(daa),
        )
    }
    /// One block at `daa`: folded, re-applied, reverted, reloaded. Refusal is returned (the block would not carry the object).
    pub fn step_at(&mut self, daa: u64, objects: &[PalwConsensusObjectV2]) -> Result<PalwStateDeltaV2, String> {
        assert!(daa > self.daa, "DAA moves forward");
        let (child, delta, skips) = self.try_fold(daa, objects).map_err(|e| format!("{e}"))?;
        assert!(skips.is_empty(), "nothing skipped at {daa}: {skips:?}");
        assert_eq!(apply_delta_v2(&self.s, &delta, &self.sp).expect("re-applies"), child, "DAA {daa}: the delta is the transition");
        assert_eq!(revert_delta_v2(&child, &delta, &self.sp).expect("reverts"), self.s, "DAA {daa}: the delta reverts");
        let reloaded = PalwStateCarriageV2::from_state(&child)
            .into_state(&self.sp, Some(child.state_root()))
            .unwrap_or_else(|e| panic!("DAA {daa}: the carriage reloads: {e}"));
        assert_eq!(reloaded, child, "DAA {daa}: reload is the state");
        self.s = child;
        self.daa = daa;
        Ok(delta)
    }
    pub fn step(&mut self, objects: &[PalwConsensusObjectV2]) -> Result<PalwStateDeltaV2, String> {
        self.step_at(self.daa + 1, objects)
    }
    pub fn must(&mut self, what: &str, objects: &[PalwConsensusObjectV2]) -> PalwStateDeltaV2 {
        let daa = self.daa + 1;
        self.step(objects).unwrap_or_else(|e| panic!("[native39] {what} at DAA {daa} refused: {e}"))
    }
    pub fn aux(&self, table: u8, key: &[u8]) -> Option<&Vec<u8>> {
        self.s.kernel_route().and_then(|r| r.aux.get(&(table, key.to_vec())))
    }
    pub fn policies(&self) -> NativeExecutionPoliciesV39 {
        self.p.palw_native_execution_v39.as_ref().unwrap().policies.clone()
    }
}

pub fn stage(action: u8, body: Vec<u8>, signer: PalwBondKeyV2) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::KernelRouteV1 {
        bytes: KernelRouteObjectV1::StageExecutionV14 { action, body }.encode(),
        signer,
        signature: Vec::new(),
    }
}

pub fn kid(bond: &PalwBondKeyV2) -> Digest {
    kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_bond_id_v1(bond)
}

// ---------------------------------------------------------------------------------------------------------------------------
// The fixture source (two blocks, one constant: no parameters, no material) and its job
// ---------------------------------------------------------------------------------------------------------------------------

pub struct Empty;
impl ParamSourceV39 for Empty {
    fn descriptor(&self, _: u16, _: Option<u16>) -> Option<ParamDescriptorV39> {
        None
    }
    fn read_range(&self, _: u16, _: Option<u16>, _: u64, _: usize) -> TirResult<Vec<i128>> {
        Err(TirError::new(TirErrorKind::Missing, "fixture has no parameters"))
    }
}
impl MaterialPageReaderV39 for Empty {
    fn page(&self, _: Digest, _: u16) -> Result<MaterialPageV39, String> {
        Err("fixture has no material".into())
    }
}

pub fn fixture_source(context: u32) -> CheckedSourceV39 {
    fixture_source_v(context, 5)
}
/// The fixture program with constant `value`: distinct values give distinct sources (and classes).
pub fn fixture_source_v(context: u32, value: i16) -> CheckedSourceV39 {
    let out = TensorTypeV39::scalar(DType::I16);
    let p = TirProgramV39 {
        version: 39,
        profile: SourceProfileV39::full_source(),
        prim_set_id: PRIM_SET_ID_V39,
        token_bound: 16,
        history_bound: 32,
        inputs: vec![],
        params: vec![],
        consts: vec![ConstDecl { dtype: DType::I16, shape: vec![], data: value.to_le_bytes().to_vec() }],
        states: vec![],
        blocks: vec![
            BlockV39 {
                name: "pre".into(),
                carry_in: vec![],
                nodes: vec![NodeV39 { prim: NodePrimV39::Frozen(Prim::Cast), inputs: vec![Ref::Const(0)], out: out.clone(), commit: true }],
                carry_out: vec![0],
            },
            BlockV39 {
                name: "post".into(),
                carry_in: vec![out.clone()],
                nodes: vec![NodeV39 { prim: NodePrimV39::Frozen(Prim::Cast), inputs: vec![Ref::CarryIn(0)], out, commit: true }],
                carry_out: vec![],
            },
        ],
        schedule: Schedule { pre: 0, layers: vec![], post: 1 },
        output: OutputDecl::Final { node: 0 },
    };
    FlatSourceWireV39 {
        version: 39,
        program_bytes: p.encode(),
        inventory_root: derive_inventory_root_v39(&p, [9; 64], &[]).unwrap(),
        real_parameters: ParamCommitmentsV1::default(),
        original_context_max: context,
        original_task_metadata: [9; 64],
        original_inventory: vec![],
    }
    .check()
    .unwrap()
}

/// The actors: the producer, the requester (another operator) and an independent finalizer (a third operator).
pub struct Actors {
    pub producer: PalwBondKeyV2,
    pub requester: PalwBondKeyV2,
    pub finalizer: PalwBondKeyV2,
}

pub fn actors(p: &Params) -> (Actors, Vec<(PalwBondKeyV2, Hash64, u64)>) {
    let bonds = genesis_bonds(p);
    assert!(bonds.len() >= 3, "the devnet base registers at least three genesis bonds");
    (Actors { producer: bonds[0].0, requester: bonds[1].0, finalizer: bonds[2].0 }, bonds)
}

pub fn operator(c: &N39, bond: &PalwBondKeyV2) -> Digest {
    c.s.bond(bond).expect("genesis bond").operator_id.as_bytes()
}

/// A complete funded job prepared off chain: the maximum, the producer's consent wire, and the class it binds.
pub struct Job {
    pub source: CheckedSourceV39,
    pub class: NativeClassWireV39,
    pub maximum: CheckedJobMaximumV39,
    pub consent: NativeConsentWireV39,
    pub contract: CheckedNativeMaximumV39,
}

pub fn prepare_job(c: &N39, a: &Actors, source: &CheckedSourceV39, max_positions: u32, nonce: u64, reward: u64, expires: u64) -> Job {
    let p = c.policies();
    let class = NativeClassWireV39::derive(source, &p, max_positions).expect("the class derives");
    let checked_class = class.check(source, &p).unwrap();
    let manifest = MaterialManifestV39 { version: 39, source: source.root(), master: [0; 64], bindings: vec![] };
    let request = JobMaximumV39 {
        version: 39,
        source: source.root(),
        requester: kid(&a.requester),
        producer: kid(&a.producer),
        nonce: misaka_palw_kernel::flat_execution11::nonce_bytes_v11(nonce),
        stop: RequestStopV39::Fixed { positions: 2, tokens: vec![0, 0] },
        input_manifest: manifest.root(),
    };
    let maximum = CheckedJobMaximumV39::check(source, &request, &manifest).expect("the maximum checks");
    let contract = contract_for(c, source, &class, &maximum);
    let consent = NativeConsentWireV39 {
        version: 39,
        class: checked_class.root(),
        requester: a.requester,
        requester_operator: operator(c, &a.requester),
        producer_operator: operator(c, &a.producer),
        reward,
        expires,
        maximum_contract: contract.record().root(),
        maximum: request_bytes(&maximum),
        material: manifest_bytes(&maximum),
    };
    Job { source: source.clone(), class, maximum, consent, contract }
}

/// The complete maximum contract the producer signs, derived exactly as the fold quotes it.
pub fn contract_for(c: &N39, source: &CheckedSourceV39, class: &NativeClassWireV39, maximum: &CheckedJobMaximumV39) -> CheckedNativeMaximumV39 {
    let p = c.policies();
    let checked_class = class.check(source, &p).unwrap();
    let quota = derive_quota_v39(source, maximum, checked_class.root(), p.root(), class.max_positions, 32).expect("the quota derives");
    let e = kernel_route_extras(&c.p, c.daa + 1);
    let ledger = c.s.kernel_route().map(|r| r.header.policy.clone()).expect("the route header exists before the consent is quoted");
    CheckedNativeMaximumV39::derive(
        &checked_class,
        maximum,
        &quota,
        &p,
        &e.native_final.as_ref().unwrap().policy,
        &ledger,
        e.native_carriage_mass.as_ref().unwrap(),
        e.native_carriage_service.as_ref().unwrap(),
    )
    .expect("the complete maximum contract derives")
}

pub fn request_bytes(m: &CheckedJobMaximumV39) -> Vec<u8> {
    m.request().to_bytes()
}
pub fn manifest_bytes(m: &CheckedJobMaximumV39) -> Vec<u8> {
    m.input_manifest().to_bytes()
}

/// The producer's actual commit for the fixed request: the trace of the real interpreter (data identity only).
pub fn produce_commit(job: &Job, id: Digest) -> NativeCommitWireV39 {
    commit_of(&job.source, &job.maximum, id)
}
pub fn commit_of(source: &CheckedSourceV39, maximum: &CheckedJobMaximumV39, id: Digest) -> NativeCommitWireV39 {
    let inputs = ActualInputResolverV39::derive(source, maximum, &[], &Empty).expect("inputs resolve");
    let summary = trace_streamed_v39(source, maximum, &inputs, &Empty, &mut IgnoreTraceV39).expect("the source runs");
    NativeCommitWireV39 { version: 39, job: id, generated: vec![], claim: summary.claim }
}

pub const TABLE: u8 = 112;
pub fn hex8(d: &Digest) -> String {
    d[..8].iter().map(|b| format!("{b:02x}")).collect()
}
pub fn key<T: borsh::BorshSerialize>(kind: u8, id: &T) -> Vec<u8> {
    borsh::to_vec(&(39u8, kind, id)).unwrap()
}
pub fn final_body(id: &Digest, mode: u8) -> Vec<u8> {
    let mut fin = 39u16.to_le_bytes().to_vec();
    fin.push(mode);
    fin.extend_from_slice(id);
    fin
}
pub fn job_body(id: &Digest) -> Vec<u8> {
    let mut raw = 39u16.to_le_bytes().to_vec();
    raw.extend_from_slice(id);
    raw
}

/// The producer registers the source and its class (41, 42), each in its own block.
pub fn register_source_and_class(c: &mut N39, producer: PalwBondKeyV2, source: &CheckedSourceV39, max_positions: u32) -> Digest {
    c.must("source 41", &[stage(41, source.wire().to_bytes(), producer)]);
    assert!(c.aux(TABLE, &key(0, &source.root())).is_some(), "the source header is written");
    let p = c.policies();
    let class = NativeClassWireV39::derive(source, &p, max_positions).unwrap();
    c.must("class 42", &[stage(42, borsh::to_vec(&class).unwrap(), producer)]);
    let id = class.check(source, &p).unwrap().root();
    assert!(c.aux(TABLE, &key(2, &id)).is_some(), "the class header is written");
    id
}

/// One job of a lockstep run.
pub struct Run {
    pub a: Actors,
    pub job: Job,
    pub id: Digest,
    pub consent: Digest,
    pub commit: NativeCommitWireV39,
}

/// A lockstep run's recorded post-states.
pub struct Life {
    pub runs: Vec<Run>,
    pub class: Digest,
    pub after_fund: (u64, PalwChainStateV2),
    pub after_commit: (u64, PalwChainStateV2),
    pub after_final: (u64, PalwChainStateV2),
}

/// Runs every pair's job in lockstep — consent, Fund, Commit and an independent Final each in one shared block — so all the
/// single credits are pending in the same queue state.
pub fn run_jobs(c: &mut N39, pairs: Vec<Actors>, source: &CheckedSourceV39, reward: u64) -> Life {
    let class = register_source_and_class(c, pairs[0].producer, source, 2);
    let expires = c.daa + 3;
    let jobs: Vec<Job> = pairs.iter().map(|a| prepare_job(c, a, source, 2, 0, reward, expires)).collect();
    let consents: Vec<_> = pairs.iter().zip(&jobs).map(|(a, j)| stage(43, borsh::to_vec(&j.consent).unwrap(), a.producer)).collect();
    c.must("consent 43", &consents);
    let consent_ids: Vec<Digest> = pairs
        .iter()
        .map(|a| {
            let ids: Vec<Digest> = borsh::from_slice(c.aux(TABLE, &key(19, &a.producer)).expect("consent catalog")).unwrap();
            assert_eq!(ids.len(), 1, "one live consent per producer");
            ids[0]
        })
        .collect();
    let funds: Vec<_> = pairs
        .iter()
        .zip(&consent_ids)
        .map(|(a, consent)| stage(44, borsh::to_vec(&NativeFundWireV39 { version: 39, consent: *consent }).unwrap(), a.requester))
        .collect();
    c.must("fund 44", &funds);
    let after_fund = (c.daa, c.s.clone());
    let ids: Vec<Digest> = pairs
        .iter()
        .map(|a| {
            let jobs: Vec<Digest> = borsh::from_slice(c.aux(TABLE, &key(22, &a.producer)).expect("producer job list")).unwrap();
            assert_eq!(jobs.len(), 1);
            jobs[0]
        })
        .collect();
    let commits: Vec<NativeCommitWireV39> = jobs.iter().zip(&ids).map(|(j, id)| produce_commit(j, *id)).collect();
    let objects: Vec<_> = pairs.iter().zip(&commits).map(|(a, w)| stage(45, borsh::to_vec(w).unwrap(), a.producer)).collect();
    c.must("commit 45", &objects);
    let after_commit = (c.daa, c.s.clone());
    let temporal = jobs[0].contract.record().temporal.clone();
    let final_after = c.daa + temporal.challenge_window + temporal.final_tail;
    eprintln!(
        "[native39] {} job(s) committed at {}, challenge {} + tail {} → Final after {final_after}",
        ids.len(),
        c.daa,
        temporal.challenge_window,
        temporal.final_tail
    );
    // The producer (or its operator) may never submit its own Final; nor may anyone before final_after.
    assert!(c.try_fold(final_after + 1, &[stage(51, final_body(&ids[0], 0), pairs[0].producer)]).is_err(), "own Final refused");
    assert!(c.try_fold(final_after, &[stage(51, final_body(&ids[0], 0), pairs[0].finalizer)]).is_err(), "early Final refused");
    let finals: Vec<_> = pairs.iter().zip(&ids).map(|(a, id)| stage(51, final_body(id, 0), a.finalizer)).collect();
    c.step_at(final_after + 1, &finals).unwrap_or_else(|e| panic!("[native39] Final 51 refused: {e}"));
    let after_final = (c.daa, c.s.clone());
    let runs = pairs
        .into_iter()
        .zip(jobs)
        .zip(ids)
        .zip(consent_ids)
        .zip(commits)
        .map(|((((a, job), id), consent), commit)| Run { a, job, id, consent, commit })
        .collect();
    Life { runs, class, after_fund, after_commit, after_final }
}

// ---------------------------------------------------------------------------------------------------------------------------
// Anchors, streams, payouts and the paying block — all through the production verifiers
// ---------------------------------------------------------------------------------------------------------------------------

pub use kaspa_consensus_core::header::Header;
pub use kaspa_consensus_core::palw_native_audit_proof_v32::{prove_native_payouts_v32, verify_native_payout_rows_v32};
pub use kaspa_consensus_core::palw_native_audit_proof_v39::{NativeAuditRowReaderV39, VerifiedNativeAuxRowsV39, verify_native_aux_stream_v39};
pub use kaspa_consensus_core::palw_native_execution_v39::NativeOriginalSourceArchiveV39;
pub use kaspa_consensus_core::palw_native_job_audit_v39::CheckedNativeJobAuditV39;
pub use kaspa_consensus_core::palw_native_redemption_v33::{
    NativePayoutStatusV39, derive_native_redemption_v39, native_payout_status_v39, prove_native_coinbase_v33, verify_native_coinbase_v33,
};
pub use kaspa_consensus_core::palw_native_state_anchor_v39::{CheckedNativeStateAnchorV39, NativeStateEnvelopeV39, check_native_state_anchor_v39};
pub use kaspa_consensus_core::palw_state_proof_v1::PalwStateOpeningV1;
pub use kaspa_consensus_core::tx::{Transaction, TransactionOutput};
pub use std::collections::BTreeMap;

pub struct AuxReader<'a>(&'a BTreeMap<(u8, Vec<u8>), Vec<u8>>);
impl NativeAuditRowReaderV39 for AuxReader<'_> {
    fn read(&self, table: u8, key: &[u8], maximum: usize) -> Result<Option<Vec<u8>>, String> {
        Ok(self.0.get(&(table, key.to_vec())).filter(|v| v.len() <= maximum).cloned())
    }
}

/// The complete original aux stream of `state` (canonical key order), authenticated by the production verifier against the
/// state's own opening. `edit` may tamper with the transported rows (the reader still serves the stored ones).
pub fn stream<'s>(
    state: &'s PalwChainStateV2,
    edit: impl FnOnce(&mut Vec<(Vec<u8>, Vec<u8>)>),
) -> Result<VerifiedNativeAuxRowsV39<AuxReader<'s>>, String> {
    let opening = PalwStateOpeningV1::of(state);
    let aux = &state.kernel_route().expect("route").aux;
    let mut rows: Vec<(Vec<u8>, Vec<u8>)> = aux.iter().map(|((t, k), v)| (borsh::to_vec(&(*t, k)).unwrap(), v.clone())).collect();
    let total = rows.len();
    edit(&mut rows);
    verify_native_aux_stream_v39(&opening, opening.root(), total, rows.into_iter().map(Ok), AuxReader(aux))
}

pub fn merkle(p: &Params, daa: u64, txs: &[Transaction]) -> Hash64 {
    if p.crescendo_activation.is_active(daa) {
        kaspa_consensus_core::merkle::calc_hash_merkle_root(txs.iter())
    } else {
        kaspa_consensus_core::merkle::calc_hash_merkle_root_pre_crescendo(txs.iter())
    }
}

/// A V2-lineage header at `daa` committing `state` (its selected parent's post-state), above `parent`, carrying `txs`.
pub fn header(p: &Params, daa: u64, parent: Option<&Header>, state: Option<&PalwChainStateV2>, txs: &[Transaction]) -> Header {
    let mut h = Header::from(&p.genesis);
    h.daa_score = daa;
    h.blue_score = daa;
    h.timestamp = 1_800_000_000_000 + daa;
    h.pow_algo_id = kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_RECEIPT_V3;
    if let Some(s) = state {
        h.palw_state_root = PalwStateOpeningV1::of(s).root();
    }
    if let Some(parent) = parent {
        h.parents_by_level = vec![vec![parent.hash]].try_into().unwrap();
    }
    if !txs.is_empty() {
        h.hash_merkle_root = merkle(p, daa, txs);
    }
    h.hash = kaspa_consensus_core::hashing::header::hash(&h);
    h
}

pub fn anchor(p: &Params, child: &Header, parent: &Header, state: &PalwChainStateV2) -> Result<CheckedNativeStateAnchorV39, String> {
    check_native_state_anchor_v39(
        child,
        child.hash,
        parent,
        parent.hash,
        p,
        &PalwStateOpeningV1::of(state),
        &NativeStateEnvelopeV39 { version: 39, leaf: vec![] },
    )
}

/// The production job audit at the state `child` commits (whose selected parent is `parent`).
pub fn audit(
    p: &Params,
    child: &Header,
    parent: &Header,
    state: &PalwChainStateV2,
    job: Digest,
    archive: Option<&dyn NativeOriginalSourceArchiveV39>,
) -> Result<CheckedNativeJobAuditV39, String> {
    let rows = stream(state, |_| {})?;
    let a = anchor(p, child, parent, state)?;
    PalwChainStateV2::native_job_audit_from_stream_v39(&rows, &a, p, job, archive)
}

pub fn coinbase(daa: u64, leading: &[(u64, Hash64)], drain: &[(u64, Hash64)]) -> Transaction {
    let outputs = leading
        .iter()
        .chain(drain)
        .map(|(v, payload)| TransactionOutput::new(*v, kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk(&payload.as_bytes())))
        .collect();
    Transaction::new(0, vec![], outputs, 0, kaspa_consensus_core::subnets::SUBNETWORK_ID_COINBASE, 0, daa.to_le_bytes().to_vec())
}

pub fn drain_prefix(s: &PalwChainStateV2) -> Vec<(u64, Hash64)> {
    s.pending_payouts_iter()
        .take(kaspa_consensus_core::palw_state_v2::PALW_V2_MAX_PAYOUTS_PER_BLOCK)
        .map(|(_, row)| (row.amount, row.payload))
        .collect()
}

/// An in-memory immutable archive (bytes only; the auditor authenticates them against the original headers).
#[derive(Default, Clone)]
pub struct MemArchive {
    pub sources: BTreeMap<Digest, Vec<u8>>,
    pub classes: BTreeMap<Digest, Vec<u8>>,
    pub consents: BTreeMap<Digest, Vec<u8>>,
}
impl NativeOriginalSourceArchiveV39 for MemArchive {
    fn source(&self, id: Digest, maximum: usize) -> Result<Option<Vec<u8>>, String> {
        Ok(self.sources.get(&id).filter(|v| v.len() <= maximum).cloned())
    }
    fn class(&self, id: Digest, maximum: usize) -> Result<Option<Vec<u8>>, String> {
        Ok(self.classes.get(&id).filter(|v| v.len() <= maximum).cloned())
    }
    fn consent(&self, job: Digest, maximum: usize) -> Result<Option<Vec<u8>>, String> {
        Ok(self.consents.get(&job).filter(|v| v.len() <= maximum).cloned())
    }
}


// ---------------------------------------------------------------------------------------------------------------------------
// Master39: a complete ordered Master over fixture sources, its components composed by the production request composer
// ---------------------------------------------------------------------------------------------------------------------------

pub use kaspa_consensus_core::palw_native_master_v39::NativeMasterRegisterV39;
pub use misaka_palw_kernel::flat_component_source39::{
    ComponentStageV39, CompleteComponentManifestV39, CompleteSourceBundleV39, OriginalFilePinV39, SourceComponentV39,
    stage_input_declarations_root_v39, stage_output_declaration_root_v39, stage_program_root_v39,
};
pub use misaka_palw_kernel::flat_invocation39::{
    MasterProducerV39, MasterPublicRequestV39, PriorComponentV39, compose_and_check_component_v39, compose_component_request_v39,
};
pub use misaka_palw_kernel::flat_pipeline39::{CheckedPipelineV39, PipelineV39};
pub use misaka_palw_tir::pipeline::{StageDecl, TripRule};

pub struct MasterPlan {
    pub owner: PalwBondKeyV2,
    pub producers: Vec<PalwBondKeyV2>,
    pub sources: Vec<CheckedSourceV39>,
    pub classes: Vec<NativeClassWireV39>,
    pub class_ids: Vec<Digest>,
    pub pipeline: CheckedPipelineV39,
    pub manifest: CompleteComponentManifestV39,
    pub request: MasterPublicRequestV39,
    pub wire: NativeMasterRegisterV39,
    pub id: Digest,
    pub reward: u64,
}

/// The complete Master an owner registers: one Fixed stage per source at its whole original context, no inputs, the owner's next
/// nonce and sequence, one producer (and reward) per stage.
#[allow(clippy::too_many_arguments)]
pub fn plan_master(
    c: &N39,
    owner: PalwBondKeyV2,
    producers: &[PalwBondKeyV2],
    sources: &[CheckedSourceV39],
    reward: u64,
    sequence: u64,
    first_nonce: u64,
    deadline: u64,
) -> MasterPlan {
    let p = c.policies();
    let classes: Vec<NativeClassWireV39> =
        sources.iter().map(|s| NativeClassWireV39::derive(s, &p, s.original_context_max()).expect("class")).collect();
    let class_ids: Vec<Digest> = classes.iter().zip(sources).map(|(w, s)| w.check(s, &p).unwrap().root()).collect();
    let programs: Vec<_> = sources.iter().map(|s| s.program().clone()).collect();
    let n = sources.len();
    let pipeline = PipelineV39 {
        version: 39,
        stages: sources
            .iter()
            .enumerate()
            .map(|(i, s)| StageDecl {
                name: format!("s{i}"),
                program: i as u16,
                trip: TripRule::Fixed { n: s.original_context_max() },
                max_trip: s.original_context_max(),
                tokens: None,
                bind: vec![],
            })
            .collect(),
        output_stage: (n - 1) as u8,
    }
    .check(&programs)
    .expect("the fixture pipeline checks");
    let names: Vec<String> = (0..n).map(|i| format!("c{i}")).collect();
    let manifest = CompleteComponentManifestV39 {
        version: 39,
        repository: "fixture/native39".into(),
        revision: [1; 20],
        original_task_metadata: [9; 64],
        original_pipeline: pipeline.root(),
        original_required_components: names.clone(),
        source_files: vec![OriginalFilePinV39 { path: "model/fixture.bin".into(), bytes: 1, sha256: [7; 32] }],
        components: names
            .iter()
            .enumerate()
            .map(|(i, name)| SourceComponentV39 { name: name.clone(), source_files: vec![0], stages: vec![i as u16] })
            .collect(),
        stages: sources
            .iter()
            .map(|s| ComponentStageV39 {
                original_source: s.root(),
                original_profile: s.profile_root(),
                original_parameter: s.parameters().root(),
                original_program: stage_program_root_v39(s.program()),
                original_inventory: s.inventory_root(),
                input_declarations: stage_input_declarations_root_v39(s.program()),
                output_declaration: stage_output_declaration_root_v39(s.program()),
                original_context_max: s.original_context_max(),
                lowered_context_max: s.original_context_max(),
            })
            .collect(),
        inputs: vec![],
    };
    let bundle = CompleteSourceBundleV39 {
        version: 39,
        manifest: manifest.to_bytes().unwrap(),
        pipeline: pipeline.wire().to_bytes().unwrap(),
        sources: sources.iter().map(|s| s.wire().to_bytes()).collect(),
    };
    let complete_source = bundle.check().expect("the complete source bundle checks").root();
    let request = MasterPublicRequestV39 {
        version: 39,
        complete_source,
        requester: kid(&owner),
        requester_operator: operator(c, &owner),
        first_nonce: misaka_palw_kernel::flat_execution11::nonce_bytes_v11(first_nonce),
        producers: producers.iter().map(|b| MasterProducerV39 { producer: kid(b), operator: operator(c, b), reward }).collect(),
        prompt: vec![],
        negative: vec![],
        source: vec![],
        key: vec![],
        steps: 0,
        scalars: vec![],
        images: vec![],
        seed: [6; 32],
        requested_new_tokens: 0,
        eos: vec![],
        finalized: vec![],
    };
    request.check(&pipeline).expect("the public request checks");
    let wire = NativeMasterRegisterV39 {
        version: 39,
        sequence,
        deadline,
        classes: class_ids.clone(),
        producer_bonds: producers.to_vec(),
        manifest: manifest.to_bytes().unwrap(),
        pipeline: pipeline.wire().to_bytes().unwrap(),
        request: request.to_bytes(),
    };
    let id = wire.root(owner);
    MasterPlan {
        owner,
        producers: producers.to_vec(),
        sources: sources.to_vec(),
        classes,
        class_ids,
        pipeline,
        manifest,
        request,
        wire,
        id,
        reward,
    }
}

/// One Master component run through the fold.
pub struct ComponentRun {
    pub stage: u16,
    pub job: Digest,
    pub consent: Digest,
    pub maximum: CheckedJobMaximumV39,
    pub contract: CheckedNativeMaximumV39,
    pub commit: NativeCommitWireV39,
    pub funded_daa: u64,
    pub committed_daa: u64,
    pub final_daa: u64,
}

pub fn row_of<T: borsh::BorshDeserialize>(s: &PalwChainStateV2, kind: u8, id: &impl borsh::BorshSerialize) -> T {
    let raw = s.kernel_route().unwrap().aux.get(&(TABLE, key(kind, id))).unwrap_or_else(|| panic!("row kind {kind} absent"));
    borsh::from_slice(raw).unwrap_or_else(|e| panic!("row kind {kind} grammar: {e}"))
}

/// The prior genuine Finals a stage's composition reads, from the folded state (books 17, credits 23).
pub fn prior_of<'a>(c: &N39, plan: &MasterPlan, runs: &'a [ComponentRun]) -> Vec<PriorComponentV39<'a>> {
    runs.iter()
        .map(|r| {
            let book: kaspa_consensus_core::palw_native_job_v39::NativeJobBookV39 = row_of(&c.s, 17, &r.job);
            let (credit, _): (Digest, kaspa_consensus_core::palw_native_job_v39::NativeSingleCreditV39) = row_of(&c.s, 23, &r.job);
            PriorComponentV39 {
                stage: r.stage,
                job: r.job,
                class: plan.class_ids[r.stage as usize],
                source: plan.sources[r.stage as usize].root(),
                claim: r.commit.claim.root(),
                final_book: book.root(),
                single_credit: credit,
                positions: r.commit.claim.positions,
                generated: &[],
            }
        })
        .collect()
}

/// Composes stage `stage` with the production composer, has its producer consent, the owner fund, the producer commit and an
/// independent bond finalize it. `finalize: false` stops after the commit.
pub fn run_component(c: &mut N39, plan: &MasterPlan, stage: u16, done: &[ComponentRun], finalizer: PalwBondKeyV2, finalize: bool) -> ComponentRun {
    let source = &plan.sources[stage as usize];
    let (maximum, invocation) = {
        let prior = prior_of(c, plan, done);
        compose_and_check_component_v39(plan.id, &plan.request, &plan.pipeline, source, stage, &prior, &[]).expect("compose39")
    };
    assert_eq!(invocation.maximum(), maximum.root(), "the composition is what the native rule accepts");
    let contract = contract_for(c, source, &plan.classes[stage as usize], &maximum);
    let producer = plan.producers[stage as usize];
    let consent = NativeConsentWireV39 {
        version: 39,
        class: plan.class_ids[stage as usize],
        requester: plan.owner,
        requester_operator: operator(c, &plan.owner),
        producer_operator: operator(c, &producer),
        reward: plan.reward,
        expires: c.daa + 3,
        maximum_contract: contract.record().root(),
        maximum: request_bytes(&maximum),
        material: manifest_bytes(&maximum),
    };
    c.must("component consent 43", &[self::stage(43, borsh::to_vec(&consent).unwrap(), producer)]);
    let consents: Vec<Digest> = row_of(&c.s, 19, &producer);
    let consent_id = *consents.last().expect("the consent");
    c.must("component fund 44", &[self::stage(44, borsh::to_vec(&NativeFundWireV39 { version: 39, consent: consent_id }).unwrap(), plan.owner)]);
    let funded_daa = c.daa;
    let jobs: Vec<Digest> = row_of(&c.s, 22, &producer);
    let job = *jobs.last().expect("the funded job");
    let commit = commit_of(source, &maximum, job);
    c.must("component commit 45", &[self::stage(45, borsh::to_vec(&commit).unwrap(), producer)]);
    let committed_daa = c.daa;
    let t = &contract.record().temporal;
    let final_daa = committed_daa + t.challenge_window + t.final_tail + 1;
    if finalize {
        c.step_at(final_daa, &[self::stage(51, final_body(&job, 0), finalizer)]).unwrap_or_else(|e| panic!("component Final 51: {e}"));
    }
    ComponentRun { stage, job, consent: consent_id, maximum, contract, commit, funded_daa, committed_daa, final_daa }
}
