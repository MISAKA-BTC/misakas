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
/// Possession seat (kind 0); seat exit / model retirement ride action 76.
pub const MODEL_SEAT_ACTION_V39: u8 = 74;
pub const MINING_COMMIT_ACTION_V39: u8 = 75;
pub const MODEL_RETIRE_ACTION_V39: u8 = 76;
pub const ISSUE_RECEIPT_ACTION_V39: u8 = 77;
/// Refused by name: a receipt quantum is spent only as a receipt block's own
/// work (header carriage `PNR9`, `palw_native_receipt_v39`), never as an object.
pub const SPEND_RECEIPT_ACTION_V39: u8 = 78;
pub const COLLECT_MINING_ACTION_V39: u8 = 79;
pub const MINING_TABLE_V39: u8 = 112;
// Table-112 kinds under the owning `(39, kind, ..)` prefix (grepped free 2026-10-15).
pub const MINING_MODEL_KIND_V39: u8 = 100;
pub const MINING_MODEL_SOURCE_KIND_V39: u8 = 101;
pub const MINING_SEAT_KIND_V39: u8 = 102;
pub const MINING_CLAIM_KIND_V39: u8 = 103;
pub const MINING_RECEIPT_KIND_V39: u8 = 104;
pub const MINING_EPOCH_KIND_V39: u8 = 106;
pub const MINING_SEAT_LIST_KIND_V39: u8 = 107;
pub const MINING_MASTER_USED_KIND_V39: u8 = 108;
pub const MINING_MODEL_SEQUENCE_KIND_V39: u8 = 109;
pub const MINING_PRODUCER_SEQUENCE_KIND_V39: u8 = 110;
pub const MINING_MODEL_COUNT_KIND_V39: u8 = 113;
pub const MINING_COLLECTED_KIND_V39: u8 = 114;
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


/// Canonical work of a complete Master's components produced by one bond,
/// from each component's ACTUAL accepted positions (its first-EOS final
/// length) and its own registered source. A Master's stage is credited at
/// most once (the fold's `(master, stage)` marker), so re-use, splitting or
/// duplication cannot add work; local court maxima never enter.
pub fn derive_component_work_v39(components: &[(&CheckedSourceV39, u32)]) -> Result<NativeInferenceWorkV39, String> {
    if components.is_empty() || components.len() > 64 {
        return Err("mining39 component set extent".into());
    }
    let mut total = NativeInferenceWorkV39 {
        version: 39,
        source: object_id(
            b"misaka-palw/native/mining-component-sources/v39",
            &components.iter().map(|(s, p)| (s.root(), *p)).collect::<Vec<_>>(),
        ),
        first_position: 0,
        end_position: 0,
        mac_equivalents: 0,
        bytes_read: 0,
        bytes_written: 0,
    };
    for (source, positions) in components {
        if *positions == 0 {
            return Err("mining39 component without accepted positions".into());
        }
        let w = derive_inference_work_v39(source, 0, *positions)?;
        total.end_position = total.end_position.checked_add(*positions).ok_or("mining39 position total overflow")?;
        total.mac_equivalents = add(total.mac_equivalents, w.mac_equivalents)?;
        total.bytes_read = add(total.bytes_read, w.bytes_read)?;
        total.bytes_written = add(total.bytes_written, w.bytes_written)?;
    }
    Ok(total)
}

// ---- Owning39 mining policy: explicit premise, finite capacities, no approved values ----

/// The honest-fraction premise, stated in the policy and checked against the
/// RFC-0018 certification threshold that decides it. Hash/signature/Merkle are
/// identity only (PRINCIPLES §8); drawn approval is sound only under this
/// premise, and the premise is a stated assumption, never a measurement.
pub const NATIVE_MINING_PREMISE_V39: &str = "native39 mining credit is premise-based: a mining claim earns a receipt only through \
RFC-0018 SecurityCertified (m-of-k positive attestations by verifiers drawn from the global bonded pool on randomness that did not \
exist at commit, with the G14 prosecution window elapsed), per claim, under the stated premise that the honest fraction of drawn \
verifier stake is at least k/m. There is no model-level review (user decision 2026-10-15, round 5); a registered model's tier is \
fixed at registration by palw_model_tier_v1. No cryptographic proof of computation is used or implied.";

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeMiningPolicyV39 {
    pub version: u16,
    pub network_domain: Digest,
    pub inference_cost_table: Digest,
    /// Explicit assumption about the drawn verifier STAKE (never measured).
    pub honest_stake_numerator: u32,
    pub honest_stake_denominator: u32,
    /// RFC-0018 positions this policy was written against: `m`, `k`, `veto`.
    /// Use requires the in-force certification policy to equal them, and the
    /// premise `honest >= k/m` (strict majority) to hold.
    pub certification_m: u16,
    pub certification_k: u16,
    pub certification_veto: u16,
    pub assumption_review: Digest,
    /// Network common work unit and slash price per MAC-equivalent (the
    /// receipt quantum and the reserved exposure); never a registrant input.
    pub network_quantum_mac: u64,
    pub network_slash_per_mac: u64,
    /// Final weight per receipt quantum (in the chain's weight unit).
    pub weight_per_quantum: u128,
    /// Possession seats: the producer's own stake reserved per seat (its own
    /// collateral, never another party's approval), the registration
    /// maturity (time only) and the seat hold after the last use.
    pub seat_stake_sompi: u64,
    pub model_maturity_daa: u64,
    pub seat_hold_daa: u64,
    /// Registrant share ceiling (permille of each receipt block's worker
    /// reward the registrant may name; the registrant's choice is at most this).
    pub maximum_registrant_share_permille: u16,
    pub epoch_daa: u64,
    /// A Master Final may be committed at most this long after it was established.
    pub claim_window_daa: u64,
    pub receipt_lifetime_daa: u64,
    pub maximum_models: u16,
    pub maximum_claims_per_model_epoch: u16,
    pub maximum_live_receipts_per_model: u16,
    pub maximum_quanta_per_receipt: u64,
    /// Actual retained source/cold-replay/approval/economics qualification
    /// root. No shipped default or producer flag supplies it.
    pub qualification: Digest,
}
impl NativeMiningPolicyV39 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/conditional-mining-policy/v39", self)
    }
    pub fn check(&self, domain: Digest) -> Result<(), String> {
        let m = self.certification_m as u64;
        let k = self.certification_k as u64;
        if self.version != 39
            || self.network_domain != domain
            || domain == [0; 64]
            || self.inference_cost_table != inference_cost_table_v39()
            || self.honest_stake_denominator == 0
            || self.honest_stake_numerator > self.honest_stake_denominator
            || self.honest_stake_numerator as u64 * 2 <= self.honest_stake_denominator as u64
            || m == 0
            || k == 0
            || k > m
            || k * 2 <= m
            // The stated premise covers the certification threshold: honest/den >= k/m.
            || (self.honest_stake_numerator as u64) * m < k * self.honest_stake_denominator as u64
            || [self.assumption_review, self.qualification].contains(&[0; 64])
            || self.network_quantum_mac == 0
            || self.network_slash_per_mac == 0
            || self.weight_per_quantum == 0
            || self.seat_stake_sompi == 0
            || self.model_maturity_daa == 0
            || self.seat_hold_daa == 0
            || self.maximum_registrant_share_permille > 1000
            || self.epoch_daa == 0
            || self.epoch_daa == u64::MAX
            || self.claim_window_daa == 0
            || self.receipt_lifetime_daa == 0
            || self.receipt_lifetime_daa == u64::MAX
            || self.maximum_models == 0
            || self.maximum_models > 64
            || self.maximum_claims_per_model_epoch == 0
            || self.maximum_claims_per_model_epoch > 1024
            || self.maximum_live_receipts_per_model == 0
            || self.maximum_live_receipts_per_model > 1024
            || self.maximum_quanta_per_receipt == 0
            || self.maximum_quanta_per_receipt > 4096
        {
            return Err("mining39 explicit qualified policy/finite capacities/honest-stake premise differs".into());
        }
        Ok(())
    }
    /// The in-force RFC-0018 certification policy must be the one this policy
    /// states its premise against.
    pub fn check_certification(
        &self,
        c: &crate::palw_security_certification_v1::PalwSecurityCertificationPolicyV1,
    ) -> Result<(), String> {
        if c.m != self.certification_m || c.k != self.certification_k || c.veto != self.certification_veto {
            return Err("mining39 premise was stated against another drawn-verifier threshold".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwNativeMiningFenceV39 {
    pub activation: crate::config::params::ForkActivation,
    pub policy: NativeMiningPolicyV39,
}
impl PalwNativeMiningFenceV39 {
    /// A structurally complete probe that no network may arm (zero premise
    /// review/qualification roots, refused by `check` and by validation).
    pub fn unapproved_probe_at(activation: crate::config::params::ForkActivation, network_domain: Digest) -> Self {
        Self {
            activation,
            policy: NativeMiningPolicyV39 {
                version: 39,
                network_domain,
                inference_cost_table: inference_cost_table_v39(),
                honest_stake_numerator: 2,
                honest_stake_denominator: 3,
                certification_m: crate::palw_security_certification_v1::PALW_CERT_M_DESIGN_V1,
                certification_k: crate::palw_security_certification_v1::PALW_CERT_K_DESIGN_V1,
                certification_veto: crate::palw_security_certification_v1::PALW_CERT_VETO_DESIGN_V1,
                assumption_review: [0; 64],
                network_quantum_mac: 1 << 30,
                network_slash_per_mac: 1,
                weight_per_quantum: 1,
                seat_stake_sompi: 1,
                model_maturity_daa: 1,
                seat_hold_daa: 1,
                maximum_registrant_share_permille: 0,
                epoch_daa: 86_400,
                claim_window_daa: 1,
                receipt_lifetime_daa: 1,
                maximum_models: 1,
                maximum_claims_per_model_epoch: 1,
                maximum_live_receipts_per_model: 1,
                maximum_quanta_per_receipt: 1,
                qualification: [0; 64],
            },
        }
    }
}
impl crate::config::params::Params {
    pub fn palw_native_mining_active_at_v39(&self, daa: u64) -> bool {
        self.palw_native_mining_v39
            .as_ref()
            .is_some_and(|f| f.activation != crate::config::params::ForkActivation::never() && f.activation.is_active(daa))
            && self.palw_native_execution_active_at_v39(daa)
    }
    /// **The measured-value gate** (testnet-12 DAA 14,000 release plan: no fence arms on provisional values). Armed, the policy
    /// must be well formed, every lane it rides must be armed at or below it (native39 with Final27 and funded execution, the OPV
    /// fence, RFC-0018 Gate C, the ADR-0176 bond budget and ADR-0179's tier fence), and — on every network but the isolated
    /// calibration profile, which exists to take the measurements — the exact policy must match a pinned
    /// [`PALW_NATIVE_MINING_MEASURED_RECORDS_V39`] record whose evidence (A6/A8 windows, independent native Final/redemption,
    /// Gate C m/k, ADR-0176 Q/B/R/F, the initial-seven launch run) and independent review digests are both present. That list is
    /// EMPTY, so every shipping network refuses it armed; a reviewed source change pins a record, never a local flag.
    pub fn validate_palw_native_mining_v39(&self) -> Result<(), crate::palw_mode_v2::PalwModeV2Error> {
        use crate::config::params::ForkActivation;
        use crate::palw_mode_v2::PalwModeV2Error;
        let Some(f) = self.palw_native_mining_v39.as_ref().filter(|f| f.activation != ForkActivation::never()) else {
            return Ok(());
        };
        let domain =
            crate::palw_attempt_v2::palw_network_domain_v2_for(self.net.to_string().as_bytes(), Some(self.genesis.hash)).as_bytes();
        f.policy.check(domain).map_err(|_| PalwModeV2Error::Invalid("native39 mining policy/premise is invalid"))?;
        let at = f.activation.daa_score();
        let below = |a: Option<ForkActivation>| a.filter(|a| *a != ForkActivation::never()).is_some_and(|a| a.daa_score() <= at);
        if !below(self.palw_native_execution_v39.as_ref().map(|f| f.activation))
            || !below(self.palw_native_final_v27.as_ref().map(|f| f.activation))
            || !below(self.palw_funded_execution_v11.as_ref().map(|f| f.activation))
            || !below(self.palw_panel_free_v1.as_ref().map(|f| f.activation))
            || !below(self.palw_independently_work_certified_v1)
            || !below(self.palw_bond_budget_v1.as_ref().map(|f| f.activation))
            || !below(self.palw_model_tier_v1.as_ref().map(|f| f.activation))
        {
            return Err(PalwModeV2Error::Invalid(
                "palw_native_mining_v39 needs native39 (with Final27, funded execution, OPV), RFC-0018 Gate C, the ADR-0176 bond \
                 budget and palw_model_tier_v1 armed at or below it",
            ));
        }
        if self.has_isolated_native_calibration_v34() {
            return Ok(());
        }
        if !palw_native_mining_policy_measured_v39(&f.policy, PALW_NATIVE_MINING_MEASURED_RECORDS_V39) {
            return Err(PalwModeV2Error::Invalid(
                "native39 mining (receipts, block opportunity, coinbase, Final weight) has no pinned measured-value record for this \
                 exact policy (PALW_NATIVE_MINING_MEASURED_RECORDS_V39 is empty): it stays dormant",
            ));
        }
        Ok(())
    }
}

/// **A measured-value record for the owning39 mining policy**: the exact policy (by root, on its network) the measurements selected,
/// with the digests of the evidence (A6/A8 two-window run, independent native Final/redemption replays, Gate C m/k, ADR-0176
/// Q/B/R/F and reservations, the initial-seven launch run) and of its independent review. Pinned only by a reviewed change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwNativeMiningMeasuredRecordV39 {
    pub network_domain: Digest,
    pub policy_root: Digest,
    pub evidence_digest: [u8; 32],
    pub review_digest: [u8; 32],
    pub harness: &'static str,
}
/// **The pinned records — EMPTY.** Nothing is measured; arming `palw_native_mining_v39` on a shipping network is refused.
pub const PALW_NATIVE_MINING_MEASURED_RECORDS_V39: &[PalwNativeMiningMeasuredRecordV39] = &[];
/// The gate: the policy's network and root equal a pinned record whose evidence and review digests are present and distinct.
pub fn palw_native_mining_policy_measured_v39(policy: &NativeMiningPolicyV39, records: &[PalwNativeMiningMeasuredRecordV39]) -> bool {
    let root = policy.root();
    records.iter().any(|r| {
        r.network_domain == policy.network_domain
            && r.policy_root == root
            && r.evidence_digest != [0; 32]
            && r.review_digest != [0; 32]
            && r.evidence_digest != r.review_digest
    })
}

/// Action 73 — register an ordinary native39 mining model over a registered
/// complete Master template. Registration is free and immutable (ADR-0175);
/// the economic class is DERIVED, never chosen by the registrant.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeMiningModelWireV39 {
    pub version: u16,
    pub template_master: Digest,
    pub sequence: u64,
    pub share_permille: u16,
}
impl NativeMiningModelWireV39 {
    pub fn from_bytes(raw: &[u8]) -> Result<Self, String> {
        if raw.len() != 2 + 64 + 8 + 2 {
            return Err("mining39 model register fixed extent".into());
        }
        let v: Self = borsh::from_slice(raw).map_err(|e| e.to_string())?;
        if v.version != 39 || v.template_master == [0; 64] || v.share_permille > 1000 {
            return Err("mining39 model register original terms".into());
        }
        Ok(v)
    }
}
/// The model line identity: complete source, ordered classes, pipeline, task.
pub fn native_mining_model_id_v39(complete_source: Digest, class_set: Digest, pipeline: Digest, task: Digest) -> Digest {
    object_id(b"misaka-palw/native/ordinary-mining-model/v39", &(complete_source, class_set, pipeline, task))
}
/// The derived economic class: the model line under the canonical cost table.
pub fn native_mining_economic_class_v39(model: Digest) -> Digest {
    object_id(b"misaka-palw/native/mining-economic-class/v39", &(model, inference_cost_table_v39()))
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeMiningModelRecordV39 {
    pub version: u16,
    pub model: Digest,
    pub economic_class: Digest,
    pub template_master: Digest,
    pub complete_source: Digest,
    pub manifest: Digest,
    pub pipeline: Digest,
    pub class_set: Digest,
    pub source_classes: Vec<Digest>,
    pub original_task: Digest,
    pub original_contexts: Vec<u32>,
    pub registrant: crate::palw_state_v2::PalwBondKeyV2,
    pub registrant_operator: Digest,
    pub share_permille: u16,
    pub admitted_daa: u64,
    pub policy: Digest,
    pub canonical_cost_table: Digest,
    pub seats: u16,
    pub live_claims: u32,
    pub live_receipts: u32,
    pub retired: Option<u64>,
}
impl NativeMiningModelRecordV39 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/original-mining-model/v39", self)
    }
    /// Registered, past its time-only registration maturity and not retired.
    /// No approval, exam, probation or promotion gates a model (user decision
    /// 2026-10-15, round 5); per-claim Gate C decides consensus credit.
    pub fn admits_claims(&self, now: u64, maturity: u64) -> bool {
        self.retired.is_none() && now >= self.admitted_daa.saturating_add(maturity)
    }
}
/// Action 74 — a possession seat (`kind 0`) by any Active matured bond.
/// Action 76 — `kind 1` seat exit, `kind 2` model retirement (registrant).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeMiningSeatWireV39 {
    pub version: u16,
    pub model: Digest,
    pub kind: u8,
}
impl NativeMiningSeatWireV39 {
    pub fn from_bytes(raw: &[u8]) -> Result<Self, String> {
        if raw.len() != 67 {
            return Err("mining39 seat/retire fixed extent".into());
        }
        let v: Self = borsh::from_slice(raw).map_err(|e| e.to_string())?;
        if v.version != 39 || v.model == [0; 64] || v.kind > 2 {
            return Err("mining39 seat/retire original identity/kind".into());
        }
        Ok(v)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeMiningSeatRecordV39 {
    pub version: u16,
    pub model: Digest,
    pub bond: crate::palw_state_v2::PalwBondKeyV2,
    pub operator: Digest,
    pub stake: u64,
    pub since: u64,
    pub last_use: u64,
    pub live_claims: u32,
}
impl NativeMiningSeatRecordV39 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/mining-possession-seat/v39", self)
    }
    pub fn budget_id(&self) -> Digest {
        object_id(b"misaka-palw/native/mining-seat-budget/v39", &(self.model, self.bond))
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
    /// Both unit values are the OWN network's common terms from the policy;
    /// neither is a registrant or producer input.
    pub fn derive(
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
        // Floor: a partial quantum is never credited (no rounding up of work).
        let quanta = u64::try_from(credited_mac / network_quantum_mac as u128).map_err(|_| "mining39 receipt quantum extent")?;
        if quanta == 0 {
            return Err("mining39 complete Master work is below one network quantum".into());
        }
        let reserved_exposure = mul(credited_mac, network_slash_per_mac as u128)?;
        Ok(Self { version: 39, model, master, work, credited_mac, quantum_mac: network_quantum_mac, quanta, reserved_exposure })
    }
}
/// Action 75 — commit one genuine complete-Master Final as a mining claim for
/// every stage the signer produced. `sequence` is the producer's next number.
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
    pub master_established: u64,
    pub complete_source: Digest,
    pub stages: Vec<u16>,
    pub producer: crate::palw_state_v2::PalwBondKeyV2,
    pub operator: Digest,
    pub admitted_daa: u64,
    pub epoch: u64,
    pub policy: Digest,
    pub work: NativeInferenceWorkV39,
    pub price: NativeMiningPriceV39,
    /// `(daa, receipt issued)` once decided; a closed claim never reopens.
    pub closed: Option<(u64, bool)>,
}
impl NativeMiningClaimRecordV39 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/original-mining-claim/v39", self)
    }
    pub fn claim_hash(&self) -> crate::Hash64 {
        crate::Hash64::from_bytes(self.id)
    }
}
pub fn native_mining_claim_id_v39(model: Digest, master: Digest, producer: &crate::palw_state_v2::PalwBondKeyV2, sequence: u64) -> Digest {
    object_id(b"misaka-palw/native/original-mining-claim-id/v39", &(model, master, producer, sequence))
}
/// Action 77 — issue the receipt of a decided claim (`kind 0`; anyone may
/// submit). Action 79 — collect (`kind 0` claim, `kind 1` receipt).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeMiningSubjectWireV39 {
    pub version: u16,
    pub kind: u8,
    pub id: Digest,
}
impl NativeMiningSubjectWireV39 {
    pub fn from_bytes(raw: &[u8]) -> Result<Self, String> {
        if raw.len() != 67 {
            return Err("mining39 subject fixed extent".into());
        }
        let v: Self = borsh::from_slice(raw).map_err(|e| e.to_string())?;
        if v.version != 39 || v.id == [0; 64] || v.kind > 1 {
            return Err("mining39 subject original identity/kind".into());
        }
        Ok(v)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeMiningReceiptRecordV39 {
    pub version: u16,
    pub id: Digest,
    pub claim: Digest,
    pub model: Digest,
    pub master: Digest,
    pub producer: crate::palw_state_v2::PalwBondKeyV2,
    pub operator: Digest,
    /// The decided RFC-0018 certification row's identity (the approval).
    pub approval: Digest,
    pub price: NativeMiningPriceV39,
    pub established_daa: u64,
    pub expires: u64,
    pub spent_quanta: u64,
    /// One bit per quantum (`quanta <= 4096`): a quantum is spent once, ever.
    pub spent: Vec<u8>,
}
impl NativeMiningReceiptRecordV39 {
    pub fn is_spent(&self, index: u32) -> bool {
        self.spent.get(index as usize / 8).is_some_and(|b| b & (1 << (index % 8)) != 0)
    }
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/conditionally-approved-mining-receipt/v39", self)
    }
}
pub fn native_mining_receipt_id_v39(claim: Digest) -> Digest {
    object_id(b"misaka-palw/native/mining-receipt-id/v39", &claim)
}
/// One actual receipt-block use (header carriage PNR9). A callback cannot mint it.
pub struct CheckedNativeMiningBlockV39 {
    pub(crate) block: crate::Hash64,
    pub(crate) model: Digest,
    pub(crate) receipt: Digest,
    pub(crate) index: u64,
    pub(crate) producer: crate::palw_state_v2::PalwBondKeyV2,
    pub(crate) weight: u128,
}
impl CheckedNativeMiningBlockV39 {
    pub fn block(&self) -> crate::Hash64 {
        self.block
    }
    pub fn model(&self) -> Digest {
        self.model
    }
    pub fn receipt(&self) -> Digest {
        self.receipt
    }
    pub fn index(&self) -> u64 {
        self.index
    }
    pub fn producer(&self) -> crate::palw_state_v2::PalwBondKeyV2 {
        self.producer
    }
    pub fn weight(&self) -> u128 {
        self.weight
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn policy() -> NativeMiningPolicyV39 {
        let mut p = PalwNativeMiningFenceV39::unapproved_probe_at(crate::config::params::ForkActivation::always(), [7; 64]).policy;
        p.assumption_review = [1; 64];
        p.qualification = [2; 64];
        p
    }
    #[test]
    fn the_probe_is_never_armable_and_the_premise_must_cover_k_over_m() {
        let probe = PalwNativeMiningFenceV39::unapproved_probe_at(crate::config::params::ForkActivation::always(), [7; 64]).policy;
        assert!(probe.check([7; 64]).is_err(), "zero premise review / qualification roots refuse");
        let p = policy();
        assert!(p.check([8; 64]).is_err(), "another network");
        // 2/3 honest against 32-of-47: 2*47 = 94 < 32*3 = 96, refused; 3/4 covers it.
        let mut weak = p.clone();
        weak.certification_m = 47;
        weak.certification_k = 32;
        assert!(weak.check([7; 64]).is_err(), "the stated honest fraction must be at least k/m");
        weak.honest_stake_numerator = 3;
        weak.honest_stake_denominator = 4;
        assert!(weak.check([7; 64]).is_ok());
        let mut big = weak;
        big.maximum_quanta_per_receipt = 4097;
        assert!(big.check([7; 64]).is_err(), "a receipt's spent bitmap stays bounded");
    }
    #[test]
    fn price_floors_to_whole_quanta_and_refuses_sub_quantum_work() {
        assert!(NativeMiningPriceV39::derive([1; 64], [2; 64], [3; 64], 99, 100, 1).is_err());
        let p = NativeMiningPriceV39::derive([1; 64], [2; 64], [3; 64], 250, 100, 3).unwrap();
        assert_eq!(p.quanta, 2, "a partial quantum is never credited");
        assert_eq!(p.reserved_exposure, 750);
    }
    #[test]
    fn mining_wires_are_fixed_extent() {
        let w = NativeMiningModelWireV39 { version: 39, template_master: [4; 64], sequence: 0, share_permille: 10 };
        let raw = borsh::to_vec(&w).unwrap();
        assert_eq!(NativeMiningModelWireV39::from_bytes(&raw).unwrap(), w);
        let mut long = raw.clone();
        long.push(0);
        assert!(NativeMiningModelWireV39::from_bytes(&long).is_err());
        let c = NativeMiningCommitWireV39 { version: 39, model: [1; 64], master: [2; 64], sequence: 3 };
        assert_eq!(NativeMiningCommitWireV39::from_bytes(&borsh::to_vec(&c).unwrap()).unwrap(), c);
        let s = NativeMiningSubjectWireV39 { version: 39, kind: 2, id: [1; 64] };
        assert!(NativeMiningSubjectWireV39::from_bytes(&borsh::to_vec(&s).unwrap()).is_err(), "no third subject kind");
    }
    #[test]
    fn a_registered_model_admits_claims_by_time_only() {
        let mut m = NativeMiningModelRecordV39 {
            version: 39,
            model: [1; 64],
            economic_class: [2; 64],
            template_master: [3; 64],
            complete_source: [4; 64],
            manifest: [5; 64],
            pipeline: [6; 64],
            class_set: [7; 64],
            source_classes: vec![[8; 64]],
            original_task: [9; 64],
            original_contexts: vec![32],
            registrant: crate::palw_state_v2::PalwBondKeyV2(crate::tx::TransactionOutpoint::new(crate::Hash64::from_bytes([1; 64]), 0)),
            registrant_operator: [10; 64],
            share_permille: 0,
            admitted_daa: 100,
            policy: [11; 64],
            canonical_cost_table: inference_cost_table_v39(),
            seats: 0,
            live_claims: 0,
            live_receipts: 0,
            retired: None,
        };
        assert!(!m.admits_claims(109, 10));
        assert!(m.admits_claims(110, 10), "no seat quorum, exam or approval gates a model");
        m.retired = Some(120);
        assert!(!m.admits_claims(130, 10));
    }
    #[test]
    fn the_measured_value_gate_needs_an_exact_pinned_reviewed_record() {
        let p = policy();
        assert!(PALW_NATIVE_MINING_MEASURED_RECORDS_V39.is_empty(), "nothing is measured: shipping networks refuse it armed");
        assert!(!palw_native_mining_policy_measured_v39(&p, PALW_NATIVE_MINING_MEASURED_RECORDS_V39));
        let good = PalwNativeMiningMeasuredRecordV39 {
            network_domain: p.network_domain,
            policy_root: p.root(),
            evidence_digest: [1; 32],
            review_digest: [2; 32],
            harness: "fixture",
        };
        assert!(palw_native_mining_policy_measured_v39(&p, &[good]));
        let mut other = p.clone();
        other.weight_per_quantum += 1;
        assert!(!palw_native_mining_policy_measured_v39(&other, &[good]), "a different policy is not the measured one");
        assert!(!palw_native_mining_policy_measured_v39(&p, &[PalwNativeMiningMeasuredRecordV39 { review_digest: [0; 32], ..good }]));
        assert!(!palw_native_mining_policy_measured_v39(&p, &[PalwNativeMiningMeasuredRecordV39 { review_digest: [1; 32], ..good }]));
        assert!(!palw_native_mining_policy_measured_v39(&p, &[PalwNativeMiningMeasuredRecordV39 { network_domain: [8; 64], ..good }]));
    }
}
