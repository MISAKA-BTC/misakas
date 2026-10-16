//! **RFC-0002 native v39 — the owning39 lifecycle through the REAL fold** (`apply_palw_transition_v7`).
//!
//! Every block is folded by the production transition and checked three ways (the delta re-applies to the parent and reverts to
//! it, the child's carriage reloads under its committed root). Nothing here calls a `cfg(test)` seam of the fold: the native39
//! actions are carried as `KernelRouteV1` objects exactly as a block carries them (the signature is the processor's check; the
//! fold reads the signer), and every audit/redemption value is derived by the production audit factories from proofs of the
//! folded state.
//!
//! **Arming.** Native39 is armed only through the EXISTING isolated calibration/test-genesis exception
//! (`local_validation_snapshot_v34` → `NativeCalibrationSnapshotV39::wrap` → `load_isolated_native_calibration_v39`): a distinct
//! devnet genesis whose params pass `validate_palw_v2`. No shipped preset is touched (`shipped_presets_have_pinned_fingerprints`
//! pins them). The Final environment's calibration record for the ordinary court profile is a FIXTURE INPUT of this isolated
//! genesis (`fixture_calibration`), not a measurement: no value here is reported as measured, and the snapshot purpose is
//! `Measurement`, which can never be A6/A8 or production evidence.
//!
//! **Processor mirror.** `kernel_route_extras` rebuilds `palw_kernel_route_extras_at` (consensus/src/.../processor/palw.rs) from
//! the same `Params` fields; the processor-only test seams (`attested_artifacts`, `test_admitted_classes`, `test_eligible`,
//! `max_adjudications_per_block`) are left empty / `None`, as in a non-test build.
//!
//! **No cryptographic proof of computation** (PRINCIPLES §8): the hashes, Merkle roots and signatures here are data identity
//! only; the Final is the escrow fold's independent-conditions Final, never a proof that the LLM computation is correct.
//!
//! Run: cargo test -p kaspa-consensus-core --test rfc0002_native39_lifecycle -- --nocapture

#[path = "dos_l5_common.rs"]
mod common;
use common::*;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::{ForkActivation, Params};
use kaspa_consensus_core::palw_kernel_route_v1::{PalwKernelOpvExtrasV1, PalwKernelRouteExtrasV1, PalwRewardGateTermsV1};
use kaspa_consensus_core::palw_native_calibration_v34::{LocalNativePurposeV34, local_validation_snapshot_v34};
use kaspa_consensus_core::palw_native_calibration_v39::{NativeCalibrationSnapshotV39, load_isolated_native_calibration_v39};
use kaspa_consensus_core::palw_native_execution_v39::{NativeClassWireV39, NativeExecutionPoliciesV39};
use kaspa_consensus_core::palw_native_final_v27::NativeFinalCalibrationV27;
use kaspa_consensus_core::palw_native_job_v39::{
    CheckedNativeMaximumV39, NativeCommitWireV39, NativeConsentV39, NativeConsentWireV39, NativeFundWireV39,
};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockWorkV3, PalwBondKeyV2, PalwChainStateV2, PalwConsensusObjectV2, PalwStateCarriageV2, PalwStateDeltaV2,
    PalwStateParamsV2, PalwStateV2Error, apply_delta_v2, apply_palw_transition_v7, revert_delta_v2,
};
use misaka_palw_kernel::flat_job39::{ActualInputResolverV39, CheckedJobMaximumV39, JobMaximumV39, MaterialManifestV39, RequestStopV39};
use misaka_palw_kernel::flat_material39::{MaterialPageReaderV39, MaterialPageV39};
use misaka_palw_kernel::flat_quota11::owning39::derive_quota_v39;
use misaka_palw_kernel::flat_source39::{CheckedSourceV39, FlatSourceWireV39, derive_inventory_root_v39};
use misaka_palw_kernel::flat_trace39::{IgnoreTraceV39, trace_streamed_v39};
use misaka_palw_kernel::hash::Digest;
use misaka_palw_kernel::route::KernelRouteObjectV1;
use misaka_palw_kernel::trace::ParamCommitmentsV1;
use misaka_palw_tir::interp_v39::{ParamDescriptorV39, ParamSourceV39};
use misaka_palw_tir::program_v2::OutputDecl;
use misaka_palw_tir::program_v39::*;
use misaka_palw_tir::{ConstDecl, DType, Prim, Ref, Schedule, TirError, TirErrorKind, TirResult};

// ---------------------------------------------------------------------------------------------------------------------------
// The isolated test genesis
// ---------------------------------------------------------------------------------------------------------------------------

/// FIXTURE input of the isolated genesis's Final environment for the ordinary court profile — NOT a measurement. The path sum
/// (1+1+1+1+0 = 4 DAA) fits the probe policy's 4-DAA challenge window, the only constraint the fold places on it; the byte/work
/// ceilings are wide so the tiny fixture source is always covered. The roots are distinct labelled stand-ins.
fn fixture_calibration() -> NativeFinalCalibrationV27 {
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
/// `rfc0002_native39_probe_horizons_admit_no_fund`). Here the answer and commit windows are 400 DAA and the interval 20,000, so
/// the same carriage fits (~0.46 of the service); every other funded/quota/discovery value is the probe value.
const TEST_DA_ANSWER: u64 = 400;
const TEST_INTERVAL: u64 = 20_000;
const TEST_HORIZON: u64 = 16_000;

fn snapshot34(base: &Params, wide: bool) -> kaspa_consensus_core::palw_native_calibration_v34::NativeCalibrationSnapshotV34 {
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
fn native39_params_with(wide: bool) -> Params {
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
fn native39_params() -> Params {
    native39_params_with(true)
}

fn network_domain(p: &Params) -> Hash64 {
    kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(p.net.to_string().as_bytes(), Some(p.genesis.hash))
}

fn active<T>(f: Option<&T>, act: impl Fn(&T) -> ForkActivation, daa: u64) -> bool {
    f.is_some_and(|f| act(f) != ForkActivation::never() && act(f).is_active(daa))
}

/// `palw_kernel_route_extras_at` rebuilt from `Params` (see the module doc).
fn kernel_route_extras(p: &Params, daa: u64) -> PalwKernelRouteExtrasV1 {
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

struct N39 {
    p: Params,
    sp: PalwStateParamsV2,
    s: PalwChainStateV2,
    daa: u64,
}

impl N39 {
    fn new() -> Self {
        Self::on(native39_params())
    }
    fn on(p: Params) -> Self {
        let sp = bundle(&p).state.clone();
        let s = genesis_state(&p);
        Self { p, sp, s, daa: 1_000 }
    }
    fn extras(&self, daa: u64) -> kaspa_consensus_core::palw_state_v2::PalwTransitionExtrasV1 {
        let mut e = extras(&self.p, daa);
        if self.p.palw_probabilistic_constraints_v1.is_some_and(|f| f.is_active(daa)) {
            e.kernel_route = Some(kernel_route_extras(&self.p, daa));
        }
        e
    }
    #[allow(clippy::type_complexity)]
    fn try_fold(
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
    fn step_at(&mut self, daa: u64, objects: &[PalwConsensusObjectV2]) -> Result<PalwStateDeltaV2, String> {
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
    fn step(&mut self, objects: &[PalwConsensusObjectV2]) -> Result<PalwStateDeltaV2, String> {
        self.step_at(self.daa + 1, objects)
    }
    fn must(&mut self, what: &str, objects: &[PalwConsensusObjectV2]) -> PalwStateDeltaV2 {
        let daa = self.daa + 1;
        self.step(objects).unwrap_or_else(|e| panic!("[native39] {what} at DAA {daa} refused: {e}"))
    }
    fn aux(&self, table: u8, key: &[u8]) -> Option<&Vec<u8>> {
        self.s.kernel_route().and_then(|r| r.aux.get(&(table, key.to_vec())))
    }
    fn policies(&self) -> NativeExecutionPoliciesV39 {
        self.p.palw_native_execution_v39.as_ref().unwrap().policies.clone()
    }
}

fn stage(action: u8, body: Vec<u8>, signer: PalwBondKeyV2) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::KernelRouteV1 {
        bytes: KernelRouteObjectV1::StageExecutionV14 { action, body }.encode(),
        signer,
        signature: Vec::new(),
    }
}

fn kid(bond: &PalwBondKeyV2) -> Digest {
    kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_bond_id_v1(bond)
}

// ---------------------------------------------------------------------------------------------------------------------------
// The fixture source (two blocks, one constant: no parameters, no material) and its job
// ---------------------------------------------------------------------------------------------------------------------------

struct Empty;
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

fn fixture_source(context: u32) -> CheckedSourceV39 {
    let out = TensorTypeV39::scalar(DType::I16);
    let p = TirProgramV39 {
        version: 39,
        profile: SourceProfileV39::full_source(),
        prim_set_id: PRIM_SET_ID_V39,
        token_bound: 16,
        history_bound: 32,
        inputs: vec![],
        params: vec![],
        consts: vec![ConstDecl { dtype: DType::I16, shape: vec![], data: 5i16.to_le_bytes().to_vec() }],
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
struct Actors {
    producer: PalwBondKeyV2,
    requester: PalwBondKeyV2,
    finalizer: PalwBondKeyV2,
}

fn actors(p: &Params) -> (Actors, Vec<(PalwBondKeyV2, Hash64, u64)>) {
    let bonds = genesis_bonds(p);
    assert!(bonds.len() >= 3, "the devnet base registers at least three genesis bonds");
    (Actors { producer: bonds[0].0, requester: bonds[1].0, finalizer: bonds[2].0 }, bonds)
}

fn operator(c: &N39, bond: &PalwBondKeyV2) -> Digest {
    c.s.bond(bond).expect("genesis bond").operator_id.as_bytes()
}

/// A complete funded job prepared off chain: the maximum, the producer's consent wire, and the class it binds.
struct Job {
    source: CheckedSourceV39,
    class: NativeClassWireV39,
    maximum: CheckedJobMaximumV39,
    consent: NativeConsentWireV39,
    contract: CheckedNativeMaximumV39,
}

fn prepare_job(c: &N39, a: &Actors, source: &CheckedSourceV39, max_positions: u32, nonce: u64, reward: u64, expires: u64) -> Job {
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
    let quota = derive_quota_v39(source, &maximum, checked_class.root(), p.root(), max_positions, 32).expect("the quota derives");
    let e = kernel_route_extras(&c.p, c.daa + 1);
    let ledger = c.s.kernel_route().map(|r| r.header.policy.clone()).expect("the route header exists before the consent is quoted");
    let contract = CheckedNativeMaximumV39::derive(
        &checked_class,
        &maximum,
        &quota,
        &p,
        &e.native_final.as_ref().unwrap().policy,
        &ledger,
        e.native_carriage_mass.as_ref().unwrap(),
        e.native_carriage_service.as_ref().unwrap(),
    )
    .expect("the complete maximum contract derives");
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

fn request_bytes(m: &CheckedJobMaximumV39) -> Vec<u8> {
    m.request().to_bytes()
}
fn manifest_bytes(m: &CheckedJobMaximumV39) -> Vec<u8> {
    m.input_manifest().to_bytes()
}

/// The producer's actual commit for the fixed request: the trace of the real interpreter (data identity only).
fn produce_commit(job: &Job, id: Digest) -> NativeCommitWireV39 {
    let inputs = ActualInputResolverV39::derive(&job.source, &job.maximum, &[], &Empty).expect("inputs resolve");
    let summary = trace_streamed_v39(&job.source, &job.maximum, &inputs, &Empty, &mut IgnoreTraceV39).expect("the source runs");
    NativeCommitWireV39 { version: 39, job: id, generated: vec![], claim: summary.claim }
}

const TABLE: u8 = 112;
fn hex8(d: &Digest) -> String {
    d[..8].iter().map(|b| format!("{b:02x}")).collect()
}
fn key<T: borsh::BorshSerialize>(kind: u8, id: &T) -> Vec<u8> {
    borsh::to_vec(&(39u8, kind, id)).unwrap()
}
fn final_body(id: &Digest, mode: u8) -> Vec<u8> {
    let mut fin = 39u16.to_le_bytes().to_vec();
    fin.push(mode);
    fin.extend_from_slice(id);
    fin
}
fn job_body(id: &Digest) -> Vec<u8> {
    let mut raw = 39u16.to_le_bytes().to_vec();
    raw.extend_from_slice(id);
    raw
}

/// The producer registers the source and its class (41, 42), each in its own block.
fn register_source_and_class(c: &mut N39, producer: PalwBondKeyV2, source: &CheckedSourceV39, max_positions: u32) -> Digest {
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
struct Run {
    a: Actors,
    job: Job,
    id: Digest,
    consent: Digest,
    commit: NativeCommitWireV39,
}

/// A lockstep run's recorded post-states.
struct Life {
    runs: Vec<Run>,
    class: Digest,
    after_fund: (u64, PalwChainStateV2),
    after_commit: (u64, PalwChainStateV2),
    after_final: (u64, PalwChainStateV2),
}

/// Runs every pair's job in lockstep — consent, Fund, Commit and an independent Final each in one shared block — so all the
/// single credits are pending in the same queue state.
fn run_jobs(c: &mut N39, pairs: Vec<Actors>, source: &CheckedSourceV39, reward: u64) -> Life {
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

use kaspa_consensus_core::header::Header;
use kaspa_consensus_core::palw_native_audit_proof_v32::{prove_native_payouts_v32, verify_native_payout_rows_v32};
use kaspa_consensus_core::palw_native_audit_proof_v39::{NativeAuditRowReaderV39, VerifiedNativeAuxRowsV39, verify_native_aux_stream_v39};
use kaspa_consensus_core::palw_native_execution_v39::NativeOriginalSourceArchiveV39;
use kaspa_consensus_core::palw_native_job_audit_v39::CheckedNativeJobAuditV39;
use kaspa_consensus_core::palw_native_redemption_v33::{
    NativePayoutStatusV39, derive_native_redemption_v39, native_payout_status_v39, prove_native_coinbase_v33, verify_native_coinbase_v33,
};
use kaspa_consensus_core::palw_native_state_anchor_v39::{CheckedNativeStateAnchorV39, NativeStateEnvelopeV39, check_native_state_anchor_v39};
use kaspa_consensus_core::palw_state_proof_v1::PalwStateOpeningV1;
use kaspa_consensus_core::tx::{Transaction, TransactionOutput};
use std::collections::BTreeMap;

struct AuxReader<'a>(&'a BTreeMap<(u8, Vec<u8>), Vec<u8>>);
impl NativeAuditRowReaderV39 for AuxReader<'_> {
    fn read(&self, table: u8, key: &[u8], maximum: usize) -> Result<Option<Vec<u8>>, String> {
        Ok(self.0.get(&(table, key.to_vec())).filter(|v| v.len() <= maximum).cloned())
    }
}

/// The complete original aux stream of `state` (canonical key order), authenticated by the production verifier against the
/// state's own opening. `edit` may tamper with the transported rows (the reader still serves the stored ones).
fn stream<'s>(
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

fn merkle(p: &Params, daa: u64, txs: &[Transaction]) -> Hash64 {
    if p.crescendo_activation.is_active(daa) {
        kaspa_consensus_core::merkle::calc_hash_merkle_root(txs.iter())
    } else {
        kaspa_consensus_core::merkle::calc_hash_merkle_root_pre_crescendo(txs.iter())
    }
}

/// A V2-lineage header at `daa` committing `state` (its selected parent's post-state), above `parent`, carrying `txs`.
fn header(p: &Params, daa: u64, parent: Option<&Header>, state: Option<&PalwChainStateV2>, txs: &[Transaction]) -> Header {
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

fn anchor(p: &Params, child: &Header, parent: &Header, state: &PalwChainStateV2) -> Result<CheckedNativeStateAnchorV39, String> {
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
fn audit(
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

fn coinbase(daa: u64, leading: &[(u64, Hash64)], drain: &[(u64, Hash64)]) -> Transaction {
    let outputs = leading
        .iter()
        .chain(drain)
        .map(|(v, payload)| TransactionOutput::new(*v, kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk(&payload.as_bytes())))
        .collect();
    Transaction::new(0, vec![], outputs, 0, kaspa_consensus_core::subnets::SUBNETWORK_ID_COINBASE, 0, daa.to_le_bytes().to_vec())
}

fn drain_prefix(s: &PalwChainStateV2) -> Vec<(u64, Hash64)> {
    s.pending_payouts_iter()
        .take(kaspa_consensus_core::palw_state_v2::PALW_V2_MAX_PAYOUTS_PER_BLOCK)
        .map(|(_, row)| (row.amount, row.payload))
        .collect()
}

/// An in-memory immutable archive (bytes only; the auditor authenticates them against the original headers).
#[derive(Default, Clone)]
struct MemArchive {
    sources: BTreeMap<Digest, Vec<u8>>,
    classes: BTreeMap<Digest, Vec<u8>>,
    consents: BTreeMap<Digest, Vec<u8>>,
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
// Tests
// ---------------------------------------------------------------------------------------------------------------------------

#[test]
fn rfc0002_native39_isolated_params_validate_and_shipped_presets_stay_dormant() {
    let p = native39_params();
    assert!(p.palw_native_execution_active_at_v39(0), "native39 is armed from the isolated genesis");
    assert!(p.palw_native_mining_v39.is_none(), "mining39 is not armed by the calibration path");
    assert!(p.has_isolated_native_calibration_v34());
    assert_ne!(p.genesis.hash, kaspa_consensus_core::config::params::devnet_shipped_params().genesis.hash, "a distinct genesis");
    for (name, shipped) in [
        ("devnet", kaspa_consensus_core::config::params::devnet_shipped_params()),
        ("t12-launch", kaspa_consensus_core::config::params::palw_t12_launch_params_v1()),
    ] {
        assert!(shipped.palw_native_execution_v39.is_none(), "{name}: native39 stays None");
        assert!(shipped.palw_native_mining_v39.is_none(), "{name}: mining39 stays None");
    }
}

/// FINDING (not fixed: a policy choice): the LOCAL validation preset copies the probe horizons, under which no native39 Fund is
/// admissible — the paged carriage's 88-packet floor in a 3-DAA window is ~60x the 2-DAA-period reserved service.
#[test]
fn rfc0002_native39_probe_horizons_admit_no_fund() {
    let mut c = N39::on(native39_params_with(false));
    let (a, _) = actors(&c.p);
    let source = fixture_source(32);
    register_source_and_class(&mut c, a.producer, &source, 2);
    let job = prepare_job(&c, &a, &source, 2, 0, 50, c.daa + 3);
    let r = job.contract.carriage().record();
    assert_eq!((r.answer_packets, r.answer_window), (88, 3), "the carriage floor and the probe's answer window");
    c.must("consent 43", &[stage(43, borsh::to_vec(&job.consent).unwrap(), a.producer)]);
    let ids: Vec<Digest> = borsh::from_slice(c.aux(TABLE, &key(19, &a.producer)).unwrap()).unwrap();
    let e = c.step(&[stage(44, borsh::to_vec(&NativeFundWireV39 { version: 39, consent: ids[0] }).unwrap(), a.requester)]).unwrap_err();
    eprintln!("[native39] probe horizons: Fund refused: {e}");
    assert!(e.contains("exceed shared physical service"), "{e}");
}

#[test]
fn rfc0002_native39_job_audit_payout_status_and_redemption_through_the_real_fold() {
    let mut c = N39::new();
    let bonds = genesis_bonds(&c.p);
    let pairs = vec![
        Actors { producer: bonds[0].0, requester: bonds[1].0, finalizer: bonds[2].0 },
        Actors { producer: bonds[3].0, requester: bonds[4].0, finalizer: bonds[2].0 },
    ];
    let source = fixture_source(32);
    let reward = 50;
    let life = run_jobs(&mut c, pairs, &source, reward);
    let (final_daa, s_final) = (life.after_final.0, life.after_final.1.clone());
    // Every single credit is written once and pending in the Final block's queue.
    for r in &life.runs {
        assert!(s_final.kernel_route().unwrap().aux.contains_key(&(TABLE, key(23, &r.id))), "credit row 23");
    }
    assert_eq!(s_final.pending_payouts_iter().count(), life.runs.len(), "one pending row per credit");
    // A second Final on a settled job is refused (the escrow settles once).
    assert!(c.try_fold(c.daa + 1, &[stage(51, final_body(&life.runs[0].id, 0), life.runs[0].a.finalizer)]).is_err(), "re-Final refused");
    // The paying block P: the fold drains the queue prefix (step 1b) at the next block.
    let paying_daa = final_daa + 1;
    c.must("paying block", &[]);
    let s_paid = c.s.clone();
    assert_eq!(s_paid.pending_payouts_iter().count(), 0, "the paying block drained every pending credit");

    // Headers: F (Final block) → P (paying block, commits S_final, carries the coinbase) → Q (commits S_paid).
    let p = c.p.clone();
    let (fund_daa, s_fund) = (life.after_fund.0, life.after_fund.1.clone());
    let (commit_daa, s_commit) = (life.after_commit.0, life.after_commit.1.clone());
    let h_fund = header(&p, fund_daa, None, None, &[]);
    let h_commit_child = header(&p, fund_daa + 1, Some(&h_fund), Some(&s_fund), &[]);
    let h_commit = header(&p, commit_daa, None, None, &[]);
    let h_after_commit = header(&p, commit_daa + 1, Some(&h_commit), Some(&s_commit), &[]);
    let h_final = header(&p, final_daa, None, None, &[]);
    let drain = drain_prefix(&s_final);
    let leading = [(1_000u64, Hash64::from_u64_word(0x5EED))];
    let cb = coinbase(paying_daa, &leading, &drain);
    let txs = vec![cb.clone()];
    let h_paying = header(&p, paying_daa, Some(&h_final), Some(&s_final), &txs);
    let h_after = header(&p, paying_daa + 1, Some(&h_paying), Some(&s_paid), &[]);

    let (ra, rb) = (&life.runs[0], &life.runs[1]);
    // ---- the job audit factory re-derives everything from the authenticated stream ----
    let fin = audit(&p, &h_paying, &h_final, &s_final, ra.id, None).expect("the Final-state job audit");
    assert_eq!(fin.terminal(), kaspa_consensus_core::palw_native_job_v39::NativeJobTerminalV39::Final);
    assert_eq!(fin.original().reward, reward);
    assert_eq!(fin.original().consent, ra.consent, "the original consent");
    assert_eq!(fin.original().producer, ra.a.producer);
    assert_eq!(fin.original().requester, ra.a.requester);
    assert_eq!(fin.maximum().root(), ra.job.maximum.root(), "the original maximum");
    assert_eq!(fin.original().maximum_contract, ra.job.contract.record().root(), "the complete maximum contract");
    assert_eq!(fin.claim().map(|c| c.claim), Some(ra.commit.claim.root()), "the committed claim");
    let st = fin.settlement().expect("the settlement");
    assert_eq!((st.producer_credit, st.requester_credit, st.actual_slash), (reward, 0, 0), "full producer credit, nothing slashed");
    let credit = fin.credit().expect("the single credit").clone();
    assert_eq!((credit.amount, credit.job, credit.established), (reward, ra.id, final_daa));
    assert_eq!(credit.recipient, ra.a.producer);
    assert_eq!(s_final.pending_payout(&credit.queue).map(|r| (r.amount, r.payload)), Some((reward, credit.payout)), "the queue row");
    assert_eq!((fin.master(), fin.stage()), ([0; 64], None), "a standalone job has no Master binding");
    eprintln!("[native39] audit: job {} Final, credit {} sompi to the producer, queue row pending", hex8(&ra.id), credit.amount);

    // A tampered or truncated stream, or a stream of another state under this anchor, is refused.
    let book_key = borsh::to_vec(&(TABLE, key(17, &ra.id))).unwrap();
    assert!(stream(&s_final, |rows| rows.iter_mut().find(|r| r.0 == book_key).unwrap().1[0] ^= 1).is_err(), "a tampered book row");
    assert!(stream(&s_final, |rows| rows.retain(|r| r.0 != book_key)).is_err(), "a withheld book row");
    assert!(stream(&s_final, |rows| rows.swap(0, 1)).is_err(), "reordered rows");
    {
        let other = stream(&s_paid, |_| {}).unwrap();
        let a = anchor(&p, &h_paying, &h_final, &s_final).unwrap();
        assert!(PalwChainStateV2::native_job_audit_from_stream_v39(&other, &a, &p, ra.id, None).is_err(), "stream/anchor mismatch");
    }
    assert!(anchor(&p, &h_paying, &h_final, &s_paid).is_err(), "a header commits one state only");

    // ---- payout status: NotSettled → Pending → ConsumedDeliveryWitnessRequired ----
    let status_at = |child: &Header, parent: &Header, s: &PalwChainStateV2| {
        let job = audit(&p, child, parent, s, ra.id, None).expect("job audit");
        let proof = prove_native_payouts_v32(s).unwrap();
        let rows = verify_native_payout_rows_v32(&proof, PalwStateOpeningV1::of(s).root()).unwrap();
        native_payout_status_v39(&job, &rows).unwrap()
    };
    let s1 = status_at(&h_after_commit, &h_commit, &s_commit);
    let s2 = status_at(&h_paying, &h_final, &s_final);
    let s3 = status_at(&h_after, &h_paying, &s_paid);
    eprintln!("[native39] payout status: committed {s1:?} → Final {s2:?} → paid {s3:?}");
    assert_eq!(s1, NativePayoutStatusV39::NotSettled);
    assert!(matches!(s2, NativePayoutStatusV39::Pending { amount, .. } if amount == reward));
    assert_eq!(s3, NativePayoutStatusV39::ConsumedDeliveryWitnessRequired, "absence is never read as delivery");
    let _ = h_commit_child; // the Fund-state anchor is exercised by the status of a funded job below
    assert_eq!(status_at(&h_commit_child, &h_fund, &s_fund), NativePayoutStatusV39::NotSettled);

    // ---- redemption: only with the bracketing witness of the paying block ----
    let proof = prove_native_coinbase_v33(&h_paying, h_paying.hash, &p, txs.iter()).unwrap();
    let paid = verify_native_coinbase_v33(&proof, &h_paying, h_paying.hash, &p).unwrap();
    let before_proof = prove_native_payouts_v32(&s_final).unwrap();
    let before = verify_native_payout_rows_v32(&before_proof, PalwStateOpeningV1::of(&s_final).root()).unwrap();
    let after_proof = prove_native_payouts_v32(&s_paid).unwrap();
    let after = verify_native_payout_rows_v32(&after_proof, PalwStateOpeningV1::of(&s_paid).root()).unwrap();
    let rank_a = s_final.pending_payouts_iter().position(|(k, _)| *k == credit.queue).unwrap();
    let index_a = (leading.len() + rank_a) as u32;
    let red = derive_native_redemption_v39(&fin, &fin, &before, &after, &paid, index_a).expect("the bracketed delivery redeems");
    assert_eq!((red.amount(), red.job(), red.credit()), (reward, ra.id, credit.root()));
    assert_eq!(red.outpoint(), kaspa_consensus_core::tx::TransactionOutpoint::new(cb.id(), index_a));
    assert_eq!((red.before_state(), red.after_state()), (before.state_root(), after.state_root()));
    eprintln!("[native39] redemption: outpoint {:?} amount {}", red.outpoint(), red.amount());
    // The second job's credit redeems from the same coinbase at its own rank.
    let fin_b = audit(&p, &h_paying, &h_final, &s_final, rb.id, None).unwrap();
    let rank_b = s_final.pending_payouts_iter().position(|(k, _)| *k == fin_b.credit().unwrap().queue).unwrap();
    assert_ne!(rank_a, rank_b);
    assert!(derive_native_redemption_v39(&fin_b, &fin_b, &before, &after, &paid, (leading.len() + rank_b) as u32).is_ok());

    // Negatives.
    // (1) Wrong witness: the after state (credit already consumed) as the "before" state.
    let after_audit = audit(&p, &h_after, &h_paying, &s_paid, ra.id, None).unwrap();
    assert!(derive_native_redemption_v39(&fin, &after_audit, &after, &after, &paid, index_a).is_err(), "post-payment state as witness");
    // (1b) A witness from before the credit existed.
    let committed = audit(&p, &h_after_commit, &h_commit, &s_commit, ra.id, None).unwrap();
    let cp = prove_native_payouts_v32(&s_commit).unwrap();
    let crows = verify_native_payout_rows_v32(&cp, PalwStateOpeningV1::of(&s_commit).root()).unwrap();
    assert!(derive_native_redemption_v39(&fin, &committed, &crows, &after, &paid, index_a).is_err(), "pre-Final witness");
    // (1c) Another job's audit as the witness.
    assert!(derive_native_redemption_v39(&fin, &fin_b, &before, &after, &paid, index_a).is_err(), "another job's credit");
    // (2) Not consumed: the after-state still holds the row.
    assert!(derive_native_redemption_v39(&fin, &fin, &before, &before, &paid, index_a).is_err(), "credit not consumed");
    // (3) A reordered drain prefix.
    let mut swapped = drain.clone();
    swapped.swap(0, 1);
    let cb_swapped = coinbase(paying_daa, &leading, &swapped);
    let h_sw = header(&p, paying_daa, Some(&h_final), Some(&s_final), std::slice::from_ref(&cb_swapped));
    let pr_sw = prove_native_coinbase_v33(&h_sw, h_sw.hash, &p, std::iter::once(&cb_swapped)).unwrap();
    let paid_sw = verify_native_coinbase_v33(&pr_sw, &h_sw, h_sw.hash, &p).unwrap();
    assert!(derive_native_redemption_v39(&fin, &fin, &before, &after, &paid_sw, index_a).is_err(), "reordered drain prefix");
    // (4) A wrong script, and (5) a wrong value, at the credit's own output.
    for (what, out) in [
        ("script", (drain[rank_a].0, Hash64::from_u64_word(0xBAD))),
        ("value", (drain[rank_a].0 + 1, drain[rank_a].1)),
    ] {
        let mut d = drain.clone();
        d[rank_a] = out;
        let cbx = coinbase(paying_daa, &leading, &d);
        let hx = header(&p, paying_daa, Some(&h_final), Some(&s_final), std::slice::from_ref(&cbx));
        let px = prove_native_coinbase_v33(&hx, hx.hash, &p, std::iter::once(&cbx)).unwrap();
        let vx = verify_native_coinbase_v33(&px, &hx, hx.hash, &p).unwrap();
        assert!(derive_native_redemption_v39(&fin, &fin, &before, &after, &vx, index_a).is_err(), "wrong {what}");
    }
    // (6) A wrong output index (the prefix starts elsewhere) and an output outside the coinbase.
    assert!(derive_native_redemption_v39(&fin, &fin, &before, &after, &paid, index_a + 7).is_err(), "wrong output index");
    // (7) A coinbase not in the pinned block.
    assert!(verify_native_coinbase_v33(&proof, &h_sw, h_sw.hash, &p).is_err(), "the coinbase leaf binds its block");
    // (8) Zero amount: a zero reward never reaches a credit — the producer's consent is refused by the fold.
    let zero = prepare_job(&c, &Actors { producer: bonds[5].0, requester: bonds[6].0, finalizer: bonds[2].0 }, &source, 2, 0, 0, c.daa + 3);
    let e = c.step(&[stage(43, borsh::to_vec(&zero.consent).unwrap(), bonds[5].0)]).unwrap_err();
    assert!(e.contains("requester/max/reward"), "a zero-reward consent is refused: {e}");
}

/// Action 53 collection, the GC'd consent and the archive: after collection the audit needs the archived original consent, which
/// it authenticates against the immutable collection record; arbitrary bytes grant nothing.
#[test]
fn rfc0002_native39_collection_gc_and_archive_audit_through_the_real_fold() {
    let mut c = N39::new();
    let bonds = genesis_bonds(&c.p);
    let pairs = vec![Actors { producer: bonds[0].0, requester: bonds[1].0, finalizer: bonds[2].0 }];
    let source = fixture_source(32);
    let life = run_jobs(&mut c, pairs, &source, 50);
    let r = &life.runs[0];
    let p = c.p.clone();
    // Archive the original bodies while they are live.
    let route = c.s.kernel_route().unwrap();
    let mut archive = MemArchive::default();
    archive.consents.insert(r.id, route.aux.get(&(TABLE, key(27, &r.id))).expect("live consent row 27").clone());
    archive.sources.insert(source.root(), route.aux.get(&(TABLE, key(1, &source.root()))).expect("source body").clone());
    archive.classes.insert(life.class, route.aux.get(&(TABLE, key(3, &life.class))).expect("class body").clone());
    let h = c.aux(TABLE, &key(16, &r.id)).unwrap().clone();
    // Collection before the complete retention horizon is refused; past it, any Active bond collects once.
    let deadline = r.job.contract.record().temporal.total + life.after_fund.0;
    assert!(c.try_fold(c.daa + 1, &[stage(53, job_body(&r.id), bonds[2].0)]).is_err(), "collection inside the retention");
    // The quota subject also closes only once its admitted interval has expired.
    let next_interval = (life.after_fund.0 / TEST_INTERVAL + 1) * TEST_INTERVAL;
    assert!(c.try_fold(deadline + 1, &[stage(53, job_body(&r.id), bonds[2].0)]).is_err(), "collection inside the admitted interval");
    c.step_at(next_interval.max(deadline + 1), &[stage(53, job_body(&r.id), bonds[2].0)])
        .unwrap_or_else(|e| panic!("collect 53 refused: {e}"));
    let s_gc = c.s.clone();
    let gc_daa = c.daa;
    assert!(c.try_fold(c.daa + 1, &[stage(53, job_body(&r.id), bonds[2].0)]).is_err(), "a second collection is refused");
    let route = s_gc.kernel_route().unwrap();
    assert!(!route.aux.contains_key(&(TABLE, key(27, &r.id))), "the consent body row is collected");
    for kind in [16u8, 17, 23, 28] {
        assert!(route.aux.contains_key(&(TABLE, key(kind, &r.id))), "kind {kind} stays");
    }
    assert_eq!(c.aux(TABLE, &key(16, &r.id)).unwrap(), &h, "the immutable funded header is kept");
    // The audit at the collected state.
    let hp = header(&p, gc_daa, None, None, &[]);
    let hc = header(&p, gc_daa + 1, Some(&hp), Some(&s_gc), &[]);
    let without = audit(&p, &hc, &hp, &s_gc, r.id, None);
    eprintln!("[native39] after collection, audit without archive: {:?}", without.as_ref().err());
    assert!(without.is_err(), "a collected consent is a named failure without its archive, never success");
    let with = audit(&p, &hc, &hp, &s_gc, r.id, Some(&archive)).expect("the archived original authenticates");
    assert_eq!(with.credit().map(|c| c.amount), Some(50));
    assert_eq!(with.original().consent, r.consent);
    // Arbitrary bytes: a changed consent body, or another job's consent, grant no authority.
    let mut bad = archive.clone();
    bad.consents.get_mut(&r.id).unwrap()[10] ^= 0xFF;
    assert!(audit(&p, &hc, &hp, &s_gc, r.id, Some(&bad)).is_err(), "changed archived bytes");
    let mut bad = archive.clone();
    bad.consents.insert(r.id, vec![0u8; 512]);
    assert!(audit(&p, &hc, &hp, &s_gc, r.id, Some(&bad)).is_err(), "arbitrary archived bytes");
    let mut bad = archive.clone();
    let body = bad.classes.get_mut(&life.class).unwrap();
    body[3] ^= 1;
    let class_live = route.aux.contains_key(&(TABLE, key(3, &life.class)));
    let res = audit(&p, &hc, &hp, &s_gc, r.id, Some(&bad));
    eprintln!("[native39] class body live after collection: {class_live}; changed archived class → {:?}", res.as_ref().err());
    if !class_live {
        assert!(res.is_err(), "a changed archived class body");
    }
}
