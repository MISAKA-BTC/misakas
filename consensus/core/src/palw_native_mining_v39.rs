//! Owning39 mining accounting. A Merkle/source identity is not a proof of LLM
//! correctness: eligibility additionally requires the consensus-owned complete
//! native Final and a randomly drawn model-epoch verifier approval under the
//! explicitly recorded honest-fraction premise. Legacy FP bytes/prices remain
//! unchanged. Court maxima are never substituted for executed inference work.
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::{
    flat_source39::CheckedSourceV39,
    hash::{Digest, object_id},
};
use misaka_palw_tir::{
    DType, Prim, Ref,
    program_v39::{DimV39, NodePrimV39, TensorTypeV39, TirProgramV39},
};
pub const REGISTER_MODEL_ACTION_V39: u8 = 73;
pub const MODEL_READY_ACTION_V39: u8 = 74;
pub const MINING_COMMIT_ACTION_V39: u8 = 75;
pub const MODEL_EPOCH_ATTEST_ACTION_V39: u8 = 76;
pub const ISSUE_RECEIPT_ACTION_V39: u8 = 77;
pub const SPEND_RECEIPT_ACTION_V39: u8 = 78;
pub const COLLECT_MINING_ACTION_V39: u8 = 79;
pub const MINING_TABLE_V39: u8 = 112;
pub const MINING_MODEL_KIND_V39: u8 = 100;
pub const MINING_MODEL_SOURCE_KIND_V39: u8 = 101;
pub const MINING_MODEL_READY_KIND_V39: u8 = 102;
pub const MINING_CLAIM_KIND_V39: u8 = 103;
pub const MINING_RECEIPT_KIND_V39: u8 = 104;
pub const MINING_SPENT_KIND_V39: u8 = 105;
pub const MINING_EPOCH_KIND_V39: u8 = 106;
pub const MINING_ATTESTATION_KIND_V39: u8 = 107;
pub const MINING_MASTER_USED_KIND_V39: u8 = 108;
pub const MINING_MODEL_SEQUENCE_KIND_V39: u8 = 109;
pub const MINING_POLICY_VERSION_V39: u16 = 39;
fn add(a: u128, b: u128) -> Result<u128, String> {
    a.checked_add(b).ok_or("mining39 work addition overflow".into())
}
fn mul(a: u128, b: u128) -> Result<u128, String> {
    a.checked_mul(b).ok_or("mining39 work product overflow".into())
}
#[derive(Clone, Copy, Default)]
struct Poly([u128; 6]);
impl Poly {
    fn fixed(n: u128) -> Self {
        let mut p = Self::default();
        p.0[0] = n;
        p
    }
    fn product(self, other: Self) -> Result<Self, String> {
        let mut out = Self::default();
        for (i, a) in self.0.iter().enumerate() {
            if *a == 0 {
                continue;
            }
            for (j, b) in other.0.iter().enumerate() {
                if *b == 0 {
                    continue;
                }
                if i + j >= 6 {
                    return Err("mining39 arithmetic polynomial degree exceeds original rank bound".into());
                }
                out.0[i + j] = add(out.0[i + j], mul(*a, *b)?)?;
            }
        }
        Ok(out)
    }
    fn scale(self, n: u128) -> Result<Self, String> {
        self.product(Self::fixed(n))
    }
    fn sum(self, first: u32, end: u32) -> Result<u128, String> {
        let mut sum = 0;
        for (k, c) in self.0.iter().enumerate() {
            if *c == 0 {
                continue;
            }
            let high = power_sum(end as u128, k)?;
            let low = power_sum(first as u128, k)?;
            sum = add(sum, mul(*c, high.checked_sub(low).ok_or("mining39 polynomial interval")?)?)?;
        }
        Ok(sum)
    }
}
fn gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    a
}
fn rational_product(mut factors: Vec<u128>, mut denominator: u128) -> Result<u128, String> {
    for n in &mut factors {
        let g = gcd(*n, denominator);
        *n /= g;
        denominator /= g;
    }
    if denominator != 1 {
        return Err("mining39 nonintegral power sum".into());
    }
    factors.into_iter().try_fold(1, mul)
}
/// Sum h^degree for h=1..=n; cancellation precedes checked products.
fn power_sum(n: u128, degree: usize) -> Result<u128, String> {
    if n == 0 {
        return Ok(0);
    }
    let next = add(n, 1)?;
    let twice = add(mul(n, 2)?, 1)?;
    match degree {
        0 => Ok(n),
        1 => rational_product(vec![n, next], 2),
        2 => rational_product(vec![n, next, twice], 6),
        3 => {
            let v = rational_product(vec![n, next], 2)?;
            mul(v, v)
        }
        4 => rational_product(
            vec![n, next, twice, add(mul(mul(n, n)?, 3)?, mul(n, 3)?)?.checked_sub(1).ok_or("mining39 fourth power coefficient")?],
            30,
        ),
        5 => rational_product(
            vec![n, n, next, next, add(mul(mul(n, n)?, 2)?, mul(n, 2)?)?.checked_sub(1).ok_or("mining39 fifth power coefficient")?],
            12,
        ),
        _ => Err("mining39 unsupported arithmetic degree".into()),
    }
}
fn dimension(d: DimV39, window: u32, from: u32) -> Poly {
    let limit = match d {
        DimV39::Fixed(n) => return Poly::fixed(n as u128),
        DimV39::H => window,
        DimV39::SelectedH(cap) => window.min(cap),
    };
    if from >= limit {
        Poly::fixed(limit as u128)
    } else {
        let mut p = Poly::default();
        p.0[1] = 1;
        p
    }
}
fn elements(t: &TensorTypeV39, window: u32, from: u32) -> Result<Poly, String> {
    t.shape.iter().try_fold(Poly::fixed(1), |p, d| p.product(dimension(*d, window, from)))
}
fn input_type(p: &TirProgramV39, block: usize, r: Ref) -> Result<TensorTypeV39, String> {
    let b = &p.blocks[block];
    Ok(match r {
        Ref::Node(i) => b.nodes.get(i as usize).ok_or("mining39 original backward reference")?.out.clone(),
        Ref::CarryIn(i) => b.carry_in.get(i as usize).ok_or("mining39 original carry reference")?.clone(),
        Ref::Param(i) => {
            let d = p.params.get(i as usize).ok_or("mining39 original parameter")?;
            TensorTypeV39::fixed(d.dtype, &d.shape)
        }
        Ref::Const(i) => {
            let d = p.consts.get(i as usize).ok_or("mining39 original constant")?;
            TensorTypeV39::fixed(d.dtype, &d.shape)
        }
        Ref::State(i) => {
            let d = p.states.get(i as usize).ok_or("mining39 original state")?;
            TensorTypeV39::fixed(d.dtype, &d.shape)
        }
        Ref::Input(i) if i < 2 => TensorTypeV39::scalar(DType::Idx),
        Ref::Input(i) => {
            let d = p.inputs.get(i as usize - 2).ok_or("mining39 original input")?;
            TensorTypeV39::fixed(d.dtype, &d.shape)
        }
    })
}
/// Structural parameter views preserve their ORIGINAL stored width. Computed
/// activations never acquire a caller-declared weight width multiplier.
fn weight_width(p: &TirProgramV39, block: usize, r: Ref) -> Option<u128> {
    match r {
        Ref::Param(i) => Some(p.params.get(i as usize)?.dtype.width() as u128),
        Ref::Node(i) => {
            let n = p.blocks.get(block)?.nodes.get(i as usize)?;
            match &n.prim {
                NodePrimV39::Frozen(
                    Prim::Cast | Prim::Reshape | Prim::Transpose { .. } | Prim::Slice { .. } | Prim::Broadcast | Prim::Gather { .. },
                ) => weight_width(p, block, *n.inputs.first()?),
                NodePrimV39::Frozen(Prim::Concat { .. }) => {
                    let mut width = None;
                    for r in &n.inputs {
                        let w = weight_width(p, block, *r)?;
                        if width.is_some_and(|v| v != w) {
                            return None;
                        }
                        width = Some(w);
                    }
                    width
                }
                _ => None,
            }
        }
        _ => None,
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeInferenceWorkV39 {
    pub version: u16,
    pub source: Digest,
    pub first_position: u32,
    pub end_position: u32,
    pub mac_equivalents: u128,
    pub bytes_read: u128,
    pub bytes_written: u128,
}
impl NativeInferenceWorkV39 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/canonical-inference-work/v39", self)
    }
}
pub fn inference_cost_table_v39() -> Digest {
    let t = &crate::palw_economic_compute_v1::PALW_ECONOMIC_COST_TABLE_V1;
    object_id(
        b"misaka-palw/native/canonical-inference-cost-table/v39",
        &(
            39u16,
            t.matmul_mac,
            t.elementwise,
            t.transcendental,
            b"owning39;all-source-nodes-bound;min(H,window/cap);original-parameter-view-width;bytes-separate".to_vec(),
        ),
    )
}
/// Exact owning39 arithmetic over [first,end). Piecewise polynomials integrate
/// every H/SelectedH breakpoint in O(nodes*breakpoints*rank), not O(context).
/// Every source node is trace-bound in39, so both Select operands are counted.
/// No court work, hash bytes, timing estimate or producer quantity enters this.
pub fn derive_inference_work_v39(source: &CheckedSourceV39, first: u32, end: u32) -> Result<NativeInferenceWorkV39, String> {
    if first > end || end > source.original_context_max() {
        return Err("mining39 work exceeds original source positions".into());
    }
    let p = source.program();
    let info = misaka_palw_tir::validate_v39::validate_v39(p)?;
    let t = &crate::palw_economic_compute_v1::PALW_ECONOMIC_COST_TABLE_V1;
    let mut result = NativeInferenceWorkV39 {
        version: 39,
        source: source.root(),
        first_position: first,
        end_position: end,
        mac_equivalents: 0,
        bytes_read: 0,
        bytes_written: 0,
    };
    if first == end {
        return Ok(result);
    }
    for (block, _) in p.occurrences() {
        let block = block as usize;
        let b = &p.blocks[block];
        let window = info.windows[block].unwrap_or(1);
        for n in &b.nodes {
            let ins = n.inputs.iter().map(|r| input_type(p, block, *r)).collect::<Result<Vec<_>, _>>()?;
            let mut starts = std::collections::BTreeSet::from([first, end]);
            for ty in ins.iter().chain(std::iter::once(&n.out)) {
                for d in &ty.shape {
                    let cap = match d {
                        DimV39::Fixed(_) => continue,
                        DimV39::H => window,
                        DimV39::SelectedH(c) => window.min(*c),
                    };
                    if cap > first && cap < end {
                        starts.insert(cap);
                    }
                }
            }
            let starts = starts.into_iter().collect::<Vec<_>>();
            for bounds in starts.windows(2) {
                let (a, z) = (bounds[0], bounds[1]);
                let out = elements(&n.out, window, a)?;
                let mut reads = 0;
                for input in &ins {
                    reads = add(reads, elements(input, window, a)?.scale(input.dtype.width() as u128)?.sum(a, z)?)?;
                }
                let mut writes = out.scale(n.out.dtype.width() as u128)?.sum(a, z)?;
                let arithmetic = match &n.prim {
                    NodePrimV39::Frozen(Prim::MatMul) => {
                        let left = ins.first().ok_or("mining39 MatMul left")?;
                        let k = *left.shape.last().ok_or("mining39 MatMul contraction")?;
                        let width = n.inputs.iter().filter_map(|r| weight_width(p, block, *r)).max().unwrap_or(1);
                        out.product(dimension(k, window, a))?.scale(mul(width, t.matmul_mac as u128)?)?.sum(a, z)?
                    }
                    NodePrimV39::Frozen(Prim::ReduceSum { .. } | Prim::ReduceMax { .. }) => {
                        elements(ins.first().ok_or("mining39 reduction input")?, window, a)?.scale(t.elementwise as u128)?.sum(a, z)?
                    }
                    NodePrimV39::Frozen(Prim::IntExp | Prim::IntLn | Prim::IntRsqrt) => {
                        out.scale(t.transcendental as u128)?.sum(a, z)?
                    }
                    NodePrimV39::Frozen(Prim::TopK { k, .. }) => elements(ins.first().ok_or("mining39 TopK input")?, window, a)?
                        .scale(mul(*k as u128, t.elementwise as u128)?)?
                        .sum(a, z)?,
                    NodePrimV39::HistorySelect { cap, .. } => elements(ins.first().ok_or("mining39 HistorySelect input")?, window, a)?
                        .product(dimension(DimV39::SelectedH(*cap), window, a))?
                        .scale(t.elementwise as u128)?
                        .sum(a, z)?,
                    NodePrimV39::Frozen(Prim::Gather { .. }) => {
                        reads = add(
                            out.scale(ins.first().ok_or("mining39 Gather input")?.dtype.width() as u128)?.sum(a, z)?,
                            elements(ins.get(1).ok_or("mining39 Gather indices")?, window, a)?
                                .scale(ins[1].dtype.width() as u128)?
                                .sum(a, z)?,
                        )?;
                        out.scale(t.elementwise as u128)?.sum(a, z)?
                    }
                    NodePrimV39::Frozen(Prim::HistAppend { .. }) => {
                        reads = writes;
                        writes = elements(ins.first().ok_or("mining39 history row")?, window, a)?
                            .scale(ins[0].dtype.width() as u128)?
                            .sum(a, z)?;
                        out.scale(t.elementwise as u128)?.sum(a, z)?
                    }
                    _ => out.scale(t.elementwise as u128)?.sum(a, z)?,
                };
                result.mac_equivalents = add(result.mac_equivalents, arithmetic)?;
                result.bytes_read = add(result.bytes_read, reads)?;
                result.bytes_written = add(result.bytes_written, writes)?;
            }
        }
    }
    Ok(result)
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeMiningPolicyV39 {
    pub version: u16,
    pub network_domain: Digest,
    pub inference_cost_table: Digest,
    /// Explicit assumption about the eligible independent verifier STAKE. It
    /// is not measured or guaranteed by a signature, hash, or the tally.
    pub honest_stake_numerator: u32,
    pub honest_stake_denominator: u32,
    pub assumption_review: Digest,
    pub committee: crate::palw_econ_contracts_v1::PalwModelVerificationGatePolicyV1,
    pub epoch_daa: u64,
    pub receipt_lifetime_daa: u64,
    pub maximum_models: u16,
    pub maximum_claims_per_model_epoch: u16,
    pub maximum_live_receipts_per_model: u16,
    /// Actual retained source/cold-replay/approval/economics qualification root.
    /// No shipped default or producer ready flag supplies this value.
    pub qualification: Digest,
}
impl NativeMiningPolicyV39 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/conditional-mining-policy/v39", self)
    }
    pub fn check(&self, domain: Digest) -> Result<(), String> {
        self.committee.validate().map_err(str::to_owned)?;
        if self.version != 39
            || self.network_domain != domain
            || domain == [0; 64]
            || self.inference_cost_table != inference_cost_table_v39()
            || self.honest_stake_denominator == 0
            || self.honest_stake_numerator > self.honest_stake_denominator
            || self.honest_stake_numerator as u64 * 2 <= self.honest_stake_denominator as u64
            || [self.assumption_review, self.qualification].contains(&[0; 64])
            || self.epoch_daa == 0
            || self.epoch_daa == u64::MAX
            || self.receipt_lifetime_daa <= self.committee.attestation_window_daa
            || self.receipt_lifetime_daa == u64::MAX
            || self.maximum_models == 0
            || self.maximum_models > 64
            || self.maximum_claims_per_model_epoch == 0
            || self.maximum_claims_per_model_epoch > 1024
            || self.maximum_live_receipts_per_model == 0
            || self.maximum_live_receipts_per_model > 1024
        {
            return Err("mining39 explicit qualified policy/finite capacities/honest-stake premise differs".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwNativeMiningFenceV39 {
    pub activation: crate::config::params::ForkActivation,
    pub policy: NativeMiningPolicyV39,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeMiningModelWireV39 {
    pub version: u16,
    pub template_master: Digest,
    pub sequence: u64,
    pub slash_value_per_pwu: u64,
    pub initial_target: u128,
    pub share_permille: u16,
    pub activation_daa: u64,
}
impl NativeMiningModelWireV39 {
    pub fn from_bytes(raw: &[u8]) -> Result<Self, String> {
        if raw.len() != 2 + 64 + 8 + 8 + 16 + 2 + 8 {
            return Err("mining39 model register fixed extent".into());
        }
        let v: Self = borsh::from_slice(raw).map_err(|e| e.to_string())?;
        if v.version != 39
            || v.template_master == [0; 64]
            || v.slash_value_per_pwu == 0
            || v.initial_target == 0
            || v.share_permille > 1000
        {
            return Err("mining39 model register original economic terms".into());
        }
        Ok(v)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeMiningModelRecordV39 {
    pub version: u16,
    pub model: Digest,
    pub economic_class: Digest,
    pub complete_source: Digest,
    pub manifest: Digest,
    pub pipeline: Digest,
    pub class_set: Digest,
    pub source_classes: Vec<Digest>,
    pub original_task: Digest,
    pub original_contexts: Vec<u32>,
    pub registrant: crate::palw_state_v2::PalwBondKeyV2,
    pub registrant_operator: Digest,
    pub admitted_daa: u64,
    pub policy: Digest,
    pub canonical_cost_table: Digest,
    pub source_bytes: u64,
    pub maximum_inference_mac: u128,
    pub canonical_trace_nodes: u64,
    pub active: bool,
}
impl NativeMiningModelRecordV39 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/original-mining-model/v39", self)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeMiningPriceV39 {
    pub version: u16,
    pub model: Digest,
    pub master: Digest,
    pub work: Digest,
    pub credited_mac: u128,
    pub quantum_mac: u64,
    pub quanta: u64,
    pub reserved_exposure: u128,
}
impl NativeMiningPriceV39 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/derived-mining-price/v39", self)
    }
    /// Both unit values are read from the OWN network's common floor by the
    /// native fold; neither is a model registrant or producer input.
    pub(crate) fn derive(
        model: Digest,
        master: Digest,
        work: Digest,
        credited_mac: u128,
        network_quantum_mac: u64,
        network_slash_per_mac: u64,
    ) -> Result<Self, String> {
        if [model, master, work].contains(&[0; 64]) || credited_mac == 0 || network_quantum_mac == 0 || network_slash_per_mac == 0 {
            return Err("mining39 common economic work unit absent".into());
        }
        let quanta =
            u64::try_from(credited_mac.div_ceil(network_quantum_mac as u128)).map_err(|_| "mining39 receipt quantum extent")?;
        let reserved_exposure = mul(credited_mac, network_slash_per_mac as u128)?;
        Ok(Self { version: 39, model, master, work, credited_mac, quantum_mac: network_quantum_mac, quanta, reserved_exposure })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeMiningCommitWireV39 {
    pub version: u16,
    pub model: Digest,
    pub master: Digest,
    pub sequence: u64,
}
impl NativeMiningCommitWireV39 {
    pub fn from_bytes(raw: &[u8]) -> Result<Self, String> {
        if raw.len() != 138 {
            return Err("mining39 commit fixed extent".into());
        }
        let v: Self = borsh::from_slice(raw).map_err(|e| e.to_string())?;
        if v.version != 39 || [v.model, v.master].contains(&[0; 64]) {
            return Err("mining39 commit original subject absent".into());
        }
        Ok(v)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeMiningClaimRecordV39 {
    pub version: u16,
    pub id: Digest,
    pub model: Digest,
    pub master: Digest,
    pub master_final: Digest,
    pub complete_source: Digest,
    pub producer: crate::palw_state_v2::PalwBondKeyV2,
    pub operator: Digest,
    pub admitted_daa: u64,
    pub epoch: u64,
    pub policy: Digest,
    pub work: Digest,
    pub price: NativeMiningPriceV39,
    pub budget_claim: Digest,
    pub expires: u64,
}
impl NativeMiningClaimRecordV39 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/original-mining-claim/v39", self)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeMiningReceiptRecordV39 {
    pub version: u16,
    pub id: Digest,
    pub claim: Digest,
    pub model: Digest,
    pub master: Digest,
    pub master_final: Digest,
    pub producer: crate::palw_state_v2::PalwBondKeyV2,
    pub operator: Digest,
    pub approval: Digest,
    pub price: NativeMiningPriceV39,
    pub established_daa: u64,
    pub expires: u64,
    pub spent_quanta: u64,
    pub spent_mac: u128,
}
impl NativeMiningReceiptRecordV39 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/conditionally-approved-mining-receipt/v39", self)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeMiningSpendWireV39 {
    pub version: u16,
    pub receipt: Digest,
    pub index: u64,
}
impl NativeMiningSpendWireV39 {
    pub fn from_bytes(raw: &[u8]) -> Result<Self, String> {
        if raw.len() != 74 {
            return Err("mining39 receipt spend fixed extent".into());
        }
        let v: Self = borsh::from_slice(raw).map_err(|e| e.to_string())?;
        if v.version != 39 || v.receipt == [0; 64] {
            return Err("mining39 receipt spend original identity absent".into());
        }
        Ok(v)
    }
}
/// Only the genuine state fold can mint this capability after re-reading the
/// original complete Final, one-credit collection and sampled epoch approval.
/// Public receipt DTOs, booleans and matching source roots cannot construct it.
pub struct CheckedNativeMiningEligibilityV39 {
    pub(crate) receipt: NativeMiningReceiptRecordV39,
}
impl CheckedNativeMiningEligibilityV39 {
    pub fn record(&self) -> &NativeMiningReceiptRecordV39 {
        &self.receipt
    }
}
/// One actual current-block receipt use. A callback cannot mint this token.
pub struct CheckedNativeMiningBlockV39 {
    pub(crate) block: Digest,
    pub(crate) model: Digest,
    pub(crate) receipt: Digest,
    pub(crate) producer: crate::palw_state_v2::PalwBondKeyV2,
    pub(crate) weight: u128,
    pub(crate) reward: u64,
}
impl CheckedNativeMiningBlockV39 {
    pub fn block(&self) -> Digest {
        self.block
    }
    pub fn model(&self) -> Digest {
        self.model
    }
    pub fn receipt(&self) -> Digest {
        self.receipt
    }
    pub fn producer(&self) -> crate::palw_state_v2::PalwBondKeyV2 {
        self.producer
    }
    pub fn weight(&self) -> u128 {
        self.weight
    }
    pub fn reward(&self) -> u64 {
        self.reward
    }
}
