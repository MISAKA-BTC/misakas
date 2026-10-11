//! Versioned off-chain replay/resource contract over the frozen flat-v5 scope.
//! Tensor-payload envelopes and bounded retries/public calls, not a complete
//! allocator, CPU/deadline or network-load certificate. No consensus grammar
//! or historical scope root is changed.
use crate::element::PreparedV1;
use crate::flat_court5::FlatSegContextV5;
use crate::flat_detect5::{FlatFindingV5, FlatPublicReadsV5, FlatReadV5, check_flat_claim_v5};
use crate::flat_scope5::{FlatResourceScopeV5, check_flat_resource_scope_v5};
use crate::hash::{Digest, object_id};
use crate::merkle3::LayoutV3;
use crate::merkle5::{FlatLeafHashV5, flat_material_bounds_v5};
use crate::seg::NodeOpeningV1;
use crate::seg_encoder::EncoderBindingV1;
use borsh::BorshSerialize;
use misaka_palw_tir::{
    Tensor,
    program::{StateKind, TirProgramV1},
};
use std::cell::Cell;

/// Native v3 responses have this bound, including the node/hash carrier.
pub const FLAT_REPLAY_REPLY_BYTES_V6: u128 = 10 << 10;
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct FlatReplayEnvelopeV6 {
    pub version: u16,
    /// Anchors the unchanged v5 program/profile/extent/terminal court scope.
    pub base_scope: Digest,
    pub max_positions: u32,
    /// Actual reference Tensor.data is i128, even when declared storage is i8.
    pub position_tensor_bytes: u128,
    pub largest_operand_tensor_bytes: u128,
    pub history_rows_tensor_bytes: u128,
    /// Raw parameter bytes hashed twice by v3, plus conservative tree framing.
    pub parameter_hash_bytes_per_attempt: u128,
    pub trace_hash_bytes_per_attempt: u128,
    pub node_reads_per_attempt: u128,
    pub hash_reads_per_attempt: u128,
    pub reply_bytes_per_attempt: u128,
}
impl FlatReplayEnvelopeV6 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/flat-replay-envelope/v6", self)
    }
}
fn add(a: u128, b: u128) -> Result<u128, String> {
    a.checked_add(b).ok_or_else(|| "replay envelope overflow".into())
}
fn mul(a: u128, b: u128) -> Result<u128, String> {
    a.checked_mul(b).ok_or_else(|| "replay envelope overflow".into())
}
/// Derive without weights, position arrays or tensor/tree allocations. Payload
/// quantities are not peak process memory: duplicated positions, transient
/// evaluator buffers, allocator capacity and acquired-model residency remain
/// separate. The parameter/hash allowance covers one replay and one filing.
pub fn flat_replay_envelope_v6(
    program: &TirProgramV1,
    base: &FlatResourceScopeV5,
    encoder: Option<&EncoderBindingV1>,
) -> Result<FlatReplayEnvelopeV6, String> {
    check_flat_resource_scope_v5(program, base.max_positions, encoder, base).map_err(|e| format!("{e:?}"))?;
    replay_costs_v6(program, base.max_positions, base.root(), encoder)
}
/// Shared reference-cost model, called only after the versioned consumer has
/// independently authenticated its complete scope. This is not admission.
pub(crate) fn replay_costs_v6(
    program: &TirProgramV1,
    max_positions: u32,
    base_scope: Digest,
    encoder: Option<&EncoderBindingV1>,
) -> Result<FlatReplayEnvelopeV6, String> {
    let pr = PreparedV1::without_scope(program, encoder)?;
    let p = max_positions - 1;
    let mut out = FlatReplayEnvelopeV6 {
        version: 6,
        base_scope,
        max_positions,
        position_tensor_bytes: 0,
        largest_operand_tensor_bytes: 0,
        history_rows_tensor_bytes: 0,
        parameter_hash_bytes_per_attempt: 0,
        trace_hash_bytes_per_attempt: 0,
        node_reads_per_attempt: add(pr.node_count as u128 * max_positions as u128, 10)?,
        hash_reads_per_attempt: 0,
        reply_bytes_per_attempt: 0,
    };
    let mut max_param_hash = 0;
    let mut max_node_hash = 0;
    for (s, (b, _)) in pr.w.occurrences.iter().enumerate() {
        for (n, node) in program.blocks[*b as usize].nodes.iter().enumerate() {
            let sh = pr.shapes(p, s as u16, n as u16);
            let len = LayoutV3::try_of(&sh.out).ok_or("invalid replay output shape")?.len;
            out.position_tensor_bytes = add(out.position_tensor_bytes, mul(len as u128, 16)?)?;
            let node_hash;
            if len >= 64 {
                let dims: Vec<_> = sh.out.iter().map(|d| *d as u64).collect();
                let m = flat_material_bounds_v5(node.out.dtype, &dims).ok_or("invalid replay material")?;
                out.hash_reads_per_attempt = out.hash_reads_per_attempt.max(m.localization_replies as u128);
                // Earlier H can be sub-64 and therefore use dual-root v3.
                // Bound that grammar independently of the largest flat value.
                node_hash = m.hash_input_bytes.max(2 * 63 * node.out.dtype.width() as u128 + 126 * 512 + 1024);
            } else {
                node_hash = v3_hash_allowance(&sh.out, node.out.dtype)?;
            }
            max_node_hash = max_node_hash.max(node_hash);
            out.trace_hash_bytes_per_attempt = add(out.trace_hash_bytes_per_attempt, mul(node_hash, max_positions as u128)?)?;
            for shape in &sh.inputs {
                let layout = LayoutV3::try_of(shape).ok_or("invalid replay operand shape")?;
                out.largest_operand_tensor_bytes = out.largest_operand_tensor_bytes.max(mul(layout.len as u128, 16)?);
            }
            for r in &node.inputs {
                let misaka_palw_tir::program::Ref::Param(i) = r else { continue };
                if encoder.is_some_and(|e| *i >= e.first_input) {
                    continue;
                }
                let param = &program.params[*i as usize];
                let shape: Vec<_> = param.shape.iter().map(|d| *d as usize).collect();
                let hash = v3_hash_allowance(&shape, param.dtype)?;
                out.parameter_hash_bytes_per_attempt = add(out.parameter_hash_bytes_per_attempt, mul(hash, max_positions as u128)?)?;
                max_param_hash = max_param_hash.max(hash);
            }
            if let misaka_palw_tir::Prim::HistAppend { state } = node.prim {
                let d = &program.states[state as usize];
                let StateKind::Hist { window } = d.kind else { return Err("history on fixed state".into()) };
                let shape: Vec<_> = d.shape.iter().map(|d| *d as usize).collect();
                let l = LayoutV3::try_of(&shape).ok_or("invalid history row")?;
                let rows = max_positions.min(window.saturating_sub(1)) as u128;
                out.history_rows_tensor_bytes = add(out.history_rows_tensor_bytes, mul(mul(l.len as u128, 16)?, rows)?)?;
            }
        }
    }
    // At most nine operands, with up to three full own parameter tree/hash
    // constructions apiece during a terminal filing (not public model delivery).
    out.parameter_hash_bytes_per_attempt = add(out.parameter_hash_bytes_per_attempt, mul(max_param_hash, 27)?)?;
    out.trace_hash_bytes_per_attempt = add(out.trace_hash_bytes_per_attempt, mul(max_node_hash, 32)?)?;
    out.reply_bytes_per_attempt = mul(add(out.node_reads_per_attempt, out.hash_reads_per_attempt)?, FLAT_REPLAY_REPLY_BYTES_V6)?;
    Ok(out)
}

fn v3_hash_allowance(shape: &[usize], dtype: misaka_palw_tir::DType) -> Result<u128, String> {
    let l = LayoutV3::try_of(shape).ok_or("invalid v3 hash shape")?;
    let leaves = add(l.leaves(0) as u128, l.leaves(1) as u128)?;
    add(add(mul(l.len as u128, 2 * dtype.width() as u128)?, mul(leaves, 512)?)?, 1024)
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlatReplayLimitsV6 {
    pub position_tensor_bytes: u128,
    pub operand_tensor_bytes: u128,
    pub history_tensor_bytes: u128,
    pub parameter_hash_bytes: u128,
    pub trace_hash_bytes: u128,
    pub node_reads: u128,
    pub hash_reads: u128,
    pub reply_bytes: u128,
    pub attempts: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FlatReplayResultV6 {
    Finding(FlatFindingV5),
    Limit(&'static str),
    Inconsistent(String),
}
/// Retain across DA replies. Starting a new session intentionally creates a
/// new local budget, not new bond credit or consensus rights.
pub struct FlatReplaySessionV6 {
    envelope: FlatReplayEnvelopeV6,
    limits: FlatReplayLimitsV6,
    attempts: u32,
    parameter_hash_bytes: u128,
    trace_hash_bytes: u128,
    node_reads: Cell<u128>,
    hash_reads: Cell<u128>,
    reply_bytes: Cell<u128>,
}
impl FlatReplaySessionV6 {
    pub fn new(envelope: FlatReplayEnvelopeV6, limits: FlatReplayLimitsV6) -> Self {
        Self {
            envelope,
            limits,
            attempts: 0,
            parameter_hash_bytes: 0,
            trace_hash_bytes: 0,
            node_reads: Cell::new(0),
            hash_reads: Cell::new(0),
            reply_bytes: Cell::new(0),
        }
    }
    pub fn attempts(&self) -> u32 {
        self.attempts
    }
    /// Authorized provider calls and worst native-carrier byte reservations,
    /// including cached/missing replies. These are not measured wire bytes.
    pub fn public_reads(&self) -> (u128, u128, u128) {
        (self.node_reads.get(), self.hash_reads.get(), self.reply_bytes.get())
    }
    pub fn reserved_hash_bytes(&self) -> (u128, u128) {
        (self.parameter_hash_bytes, self.trace_hash_bytes)
    }
    pub fn check(
        &mut self,
        c: &FlatSegContextV5<'_>,
        base: &FlatResourceScopeV5,
        public: &dyn FlatPublicReadsV5,
        artifact: &dyn Fn(u16, Option<u16>) -> Option<Tensor>,
        tokens: &[u32],
    ) -> FlatReplayResultV6 {
        let expected = match flat_replay_envelope_v6(c.claim.program, base, c.claim.encoder.as_ref()) {
            Ok(v) => v,
            Err(e) => return FlatReplayResultV6::Inconsistent(e),
        };
        if self.envelope != expected || c.claim.positions > expected.max_positions {
            return FlatReplayResultV6::Inconsistent("replay envelope differs from independently derived class scope".into());
        }
        for (dimension, actual, cap) in [
            ("position tensor bytes", expected.position_tensor_bytes, self.limits.position_tensor_bytes),
            ("operand tensor bytes", expected.largest_operand_tensor_bytes, self.limits.operand_tensor_bytes),
            ("history tensor bytes", expected.history_rows_tensor_bytes, self.limits.history_tensor_bytes),
        ] {
            if actual > cap {
                return FlatReplayResultV6::Limit(dimension);
            }
        }
        if self.attempts >= self.limits.attempts {
            return FlatReplayResultV6::Limit("attempts");
        }
        let Some(hash) = self.parameter_hash_bytes.checked_add(expected.parameter_hash_bytes_per_attempt) else {
            return FlatReplayResultV6::Limit("parameter hash bytes");
        };
        if hash > self.limits.parameter_hash_bytes {
            return FlatReplayResultV6::Limit("parameter hash bytes");
        }
        let Some(trace_hash) = self.trace_hash_bytes.checked_add(expected.trace_hash_bytes_per_attempt) else {
            return FlatReplayResultV6::Limit("trace hash bytes");
        };
        if trace_hash > self.limits.trace_hash_bytes {
            return FlatReplayResultV6::Limit("trace hash bytes");
        }
        if self.node_reads.get() >= self.limits.node_reads {
            return FlatReplayResultV6::Limit("node reads");
        }
        if self.reply_bytes.get().checked_add(FLAT_REPLAY_REPLY_BYTES_V6).is_none_or(|b| b > self.limits.reply_bytes) {
            return FlatReplayResultV6::Limit("reply bytes");
        }
        self.trace_hash_bytes = trace_hash;
        self.attempts += 1;
        self.parameter_hash_bytes = hash; // Reserve even an inconsistent/withheld attempt.
        struct Bounded<'a> {
            session: &'a FlatReplaySessionV6,
            public: &'a dyn FlatPublicReadsV5,
            hit: Cell<Option<&'static str>>,
        }
        impl Bounded<'_> {
            fn take(&self, count: &Cell<u128>, cap: u128, dimension: &'static str) -> bool {
                if count.get() >= cap {
                    self.hit.set(Some(dimension));
                    return false;
                }
                let Some(bytes) = self.session.reply_bytes.get().checked_add(FLAT_REPLAY_REPLY_BYTES_V6) else {
                    self.hit.set(Some("reply bytes"));
                    return false;
                };
                if bytes > self.session.limits.reply_bytes {
                    self.hit.set(Some("reply bytes"));
                    return false;
                }
                count.set(count.get() + 1);
                self.session.reply_bytes.set(bytes);
                true
            }
        }
        impl FlatPublicReadsV5 for Bounded<'_> {
            fn node(&self, u: FlatReadV5) -> Option<NodeOpeningV1> {
                self.take(&self.session.node_reads, self.session.limits.node_reads, "node reads")
                    .then(|| self.public.node(u))
                    .flatten()
            }
            fn hash(&self, u: FlatReadV5) -> Option<FlatLeafHashV5> {
                self.take(&self.session.hash_reads, self.session.limits.hash_reads, "hash reads")
                    .then(|| self.public.hash(u))
                    .flatten()
            }
        }
        let bounded = Bounded { session: self, public, hit: Cell::new(None) };
        let result = check_flat_claim_v5(c, &bounded, artifact, tokens);
        match bounded.hit.get() {
            Some(d) => FlatReplayResultV6::Limit(d),
            None => FlatReplayResultV6::Finding(result),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::element::SegClaimContextV1;
    use crate::flat_court4::tests::{identity, state_fixture};
    use crate::flat_court5::{flat_profile_commitment_v5, flat_trace_profile_id_v5};
    use crate::flat_scope5::flat_resource_scope_v5;
    use crate::seg::{SegmentedCommitmentsV1, prompt_root_of_ids_v1};
    use crate::trace::{ParamCommitmentsV1, trace_v1};
    use misaka_palw_tir::{
        DType, MapParams, TensorType,
        builder::ProgramBuilder,
        program::{HISTORY_BOUND_V1_HELD, Ref},
    };

    fn limits(e: &FlatReplayEnvelopeV6, attempts: u32) -> FlatReplayLimitsV6 {
        FlatReplayLimitsV6 {
            position_tensor_bytes: e.position_tensor_bytes,
            operand_tensor_bytes: e.largest_operand_tensor_bytes,
            history_tensor_bytes: e.history_rows_tensor_bytes,
            parameter_hash_bytes: e.parameter_hash_bytes_per_attempt * attempts as u128,
            trace_hash_bytes: e.trace_hash_bytes_per_attempt * attempts as u128,
            node_reads: e.node_reads_per_attempt * attempts as u128,
            hash_reads: e.hash_reads_per_attempt * attempts as u128,
            reply_bytes: e.reply_bytes_per_attempt * attempts as u128,
            attempts,
        }
    }
    struct Trial {
        p: TirProgramV1,
        m: MapParams,
        pc: ParamCommitmentsV1,
        base: FlatResourceScopeV5,
        envelope: FlatReplayEnvelopeV6,
        c: SegmentedCommitmentsV1,
        roots: Vec<Digest>,
    }
    impl Trial {
        fn of() -> Self {
            let (mut p, m) = identity(129);
            p.token_bound = 256;
            let pc = ParamCommitmentsV1::of_v3(&m);
            let values = trace_v1(&p, &m, &[1]).unwrap().values;
            let c = SegmentedCommitmentsV1::new(
                values
                    .iter()
                    .map(|p| p.iter().map(|o| o.iter().map(|t| flat_profile_commitment_v5(t).unwrap()).collect()).collect())
                    .collect(),
            );
            let roots = c.segment_roots();
            let base = flat_resource_scope_v5(&p, 1, None).unwrap();
            let envelope = flat_replay_envelope_v6(&p, &base, None).unwrap();
            Self { p, m, pc, base, envelope, c, roots }
        }
        fn ctx(&self) -> FlatSegContextV5<'_> {
            FlatSegContextV5 {
                profile: flat_trace_profile_id_v5(),
                claim: SegClaimContextV1 {
                    program: &self.p,
                    params: &self.pc,
                    segment_roots: &self.roots,
                    positions: 1,
                    prompt_len: 1,
                    prompt_root: prompt_root_of_ids_v1(&[1]),
                    inline_prompt: Some(&[1]),
                    generated: &[126],
                    decode: crate::job::DecodeRuleV1::Greedy,
                    encoder: None,
                },
            }
        }
    }
    struct Public<'a> {
        trial: &'a Trial,
        missing: bool,
        calls: Cell<u32>,
    }
    impl FlatPublicReadsV5 for Public<'_> {
        fn node(&self, u: FlatReadV5) -> Option<NodeOpeningV1> {
            self.calls.set(self.calls.get() + 1);
            if self.missing { None } else { self.trial.c.node_opening(u.position, u.occurrence, u.node) }
        }
        fn hash(&self, _: FlatReadV5) -> Option<FlatLeafHashV5> {
            panic!("an honest replay needs no public hash leaves")
        }
    }
    #[test]
    fn replay_v6_tensor_prices_use_i128_and_bind_unchanged_v5_scope() {
        let t = Trial::of();
        assert_eq!(t.envelope.position_tensor_bytes, 2 * 129 * 16);
        assert_eq!(t.envelope.largest_operand_tensor_bytes, 129 * 16);
        assert_eq!(t.envelope.base_scope, t.base.root());
        assert_eq!(t.envelope.node_reads_per_attempt, 12);
        assert_eq!(t.envelope.hash_reads_per_attempt, 2);
        assert_eq!(t.envelope.reply_bytes_per_attempt, 14 * FLAT_REPLAY_REPLY_BYTES_V6);
        let (p, _, _, _) = state_fixture();
        let base = flat_resource_scope_v5(&p, 4, None).unwrap();
        let e = flat_replay_envelope_v6(&p, &base, None).unwrap();
        assert_eq!(e.history_rows_tensor_bytes, 129 * 16); // window=2: one retained row
        assert!(e.trace_hash_bytes_per_attempt > base.flat_hash_input_bytes_per_claim);
        let mut bad = base.clone();
        bad.trace_value_bytes_per_position -= 1;
        assert!(flat_replay_envelope_v6(&p, &bad, None).is_err());
    }
    #[test]
    fn replay_v6_budget_rejects_before_acquired_model_or_public_access() {
        let t = Trial::of();
        let public = Public { trial: &t, missing: false, calls: Cell::new(0) };
        for dimension in 0..8 {
            let mut cap = limits(&t.envelope, 1);
            match dimension {
                0 => cap.position_tensor_bytes = 0,
                1 => cap.operand_tensor_bytes = 0,
                2 => cap.parameter_hash_bytes = 0,
                3 => cap.trace_hash_bytes = 0,
                4 => cap.node_reads = 0,
                5 => cap.reply_bytes = 0,
                6 => cap.attempts = 0,
                _ => cap.position_tensor_bytes = t.envelope.position_tensor_bytes - 1,
            }
            let mut session = FlatReplaySessionV6::new(t.envelope.clone(), cap);
            assert!(matches!(
                session.check(&t.ctx(), &t.base, &public, &|_, _| panic!("preflight must precede model access"), &[1]),
                FlatReplayResultV6::Limit(_)
            ));
            assert_eq!(session.attempts(), 0);
            assert_eq!(public.calls.get(), 0);
        }
        for i in 0..5 {
            let mut e = t.envelope.clone();
            match i {
                0 => e.version = 5,
                1 => e.base_scope[0] ^= 1,
                2 => e.parameter_hash_bytes_per_attempt = 0,
                3 => e.position_tensor_bytes = 0,
                _ => e.node_reads_per_attempt = 0,
            }
            let mut session = FlatReplaySessionV6::new(e, limits(&t.envelope, 1));
            assert!(matches!(
                session.check(&t.ctx(), &t.base, &public, &|_, _| panic!("forged declaration must not run"), &[1]),
                FlatReplayResultV6::Inconsistent(_)
            ));
        }
    }
    #[test]
    fn replay_v6_lifetime_counts_cached_reads_and_withheld_retries() {
        let t = Trial::of();
        let source = |i, l| t.m.tensors.get(&(i, l)).cloned();
        let public = Public { trial: &t, missing: true, calls: Cell::new(0) };
        let mut session = FlatReplaySessionV6::new(t.envelope.clone(), limits(&t.envelope, 2));
        for _ in 0..2 {
            assert!(matches!(
                session.check(&t.ctx(), &t.base, &public, &source, &[1]),
                FlatReplayResultV6::Finding(FlatFindingV5::Demand(_))
            ));
        }
        assert_eq!(
            session.check(&t.ctx(), &t.base, &public, &|_, _| panic!("spent retries"), &[1]),
            FlatReplayResultV6::Limit("attempts")
        );
        assert_eq!(public.calls.get(), 2);
        assert_eq!(session.public_reads(), (2, 0, 2 * FLAT_REPLAY_REPLY_BYTES_V6));
        let public = Public { trial: &t, missing: false, calls: Cell::new(0) };
        let mut cap = limits(&t.envelope, 3);
        cap.node_reads = 3;
        let mut session = FlatReplaySessionV6::new(t.envelope.clone(), cap);
        assert_eq!(session.check(&t.ctx(), &t.base, &public, &source, &[1]), FlatReplayResultV6::Finding(FlatFindingV5::Clean));
        assert_eq!(session.check(&t.ctx(), &t.base, &public, &source, &[1]), FlatReplayResultV6::Limit("node reads"));
        assert_eq!(public.calls.get(), 3);
        assert_eq!(session.public_reads(), (3, 0, 3 * FLAT_REPLAY_REPLY_BYTES_V6));
    }
    #[test]
    fn replay_v6_huge_acquired_parameters_are_priced_without_images() {
        let mut pb = ProgramBuilder::new(4, HISTORY_BOUND_V1_HELD);
        let table = pb.param("large.source", DType::I8, &[1 << 24], false);
        let ids = pb.konst(DType::Idx, &[129], &vec![0; 129]);
        let mut pre = pb.block("pre", vec![]);
        let x = pre.gather(table, ids, 0, 0);
        let pre = pre.finish(&[x]);
        let mut post = pb.block("post", vec![TensorType::fixed(DType::I8, &[129])]);
        let y = post.cast(Ref::CarryIn(0), DType::I8);
        post.commit(y);
        let post = post.finish(&[]);
        let p = pb.finish(pre, vec![], post, 0);
        let base = flat_resource_scope_v5(&p, HISTORY_BOUND_V1_HELD, None).unwrap();
        let e = flat_replay_envelope_v6(&p, &base, None).unwrap();
        assert_eq!(e.largest_operand_tensor_bytes, (1 << 24) * 16);
        assert_eq!(e.position_tensor_bytes, 258 * 16);
        assert!(e.parameter_hash_bytes_per_attempt > (1u128 << 24) * HISTORY_BOUND_V1_HELD as u128 * 2);
        // No source callback, weight image, roots or position array is needed.
    }

    #[test]
    fn replay_v6_tensor_payload_envelope_covers_frozen_primitive_and_history_traces() {
        for tag in 0..24 {
            let (p, m) = if tag == 23 {
                let (p, m, _, _) = state_fixture();
                (p, m)
            } else {
                let (p, m, _) = crate::flat_court4::tests::primitive_fixture(tag);
                (p, m)
            };
            let base = flat_resource_scope_v5(&p, 4, None).unwrap();
            let e = flat_replay_envelope_v6(&p, &base, None).unwrap();
            let trace = trace_v1(&p, &m, &[1, 2, 3, 1]).unwrap();
            for values in &trace.values {
                let bytes: u128 =
                    values.iter().flat_map(|o| o.iter()).map(|t| (t.data.len() * std::mem::size_of::<i128>()) as u128).sum();
                assert!(bytes <= e.position_tensor_bytes, "tag {tag}");
            }
            for t in m.tensors.values() {
                assert!((t.data.len() * 16) as u128 <= e.largest_operand_tensor_bytes, "tag {tag}");
            }
        }
    }

    #[test]
    fn replay_v6_hash_and_history_limits_are_explicit_and_stop_before_delegation() {
        let mut t = Trial::of();
        let mut tensor = t.m.tensors[&(0, None)].clone();
        tensor.data[128] += 1;
        t.c.commitments[0][0][0] = flat_profile_commitment_v5(&tensor).unwrap();
        t.c = SegmentedCommitmentsV1::new(t.c.commitments.clone());
        t.roots = t.c.segment_roots();
        let public = Public { trial: &t, missing: false, calls: Cell::new(0) };
        let mut cap = limits(&t.envelope, 1);
        cap.hash_reads = 0;
        let mut session = FlatReplaySessionV6::new(t.envelope.clone(), cap);
        assert_eq!(
            session.check(&t.ctx(), &t.base, &public, &|i, l| t.m.tensors.get(&(i, l)).cloned(), &[1]),
            FlatReplayResultV6::Limit("hash reads")
        );
        assert_eq!(public.calls.get(), 1);
        assert_eq!(session.public_reads(), (1, 0, FLAT_REPLAY_REPLY_BYTES_V6));
        let (p, _, _, _) = state_fixture();
        let base = flat_resource_scope_v5(&p, 4, None).unwrap();
        let e = flat_replay_envelope_v6(&p, &base, None).unwrap();
        let pc = ParamCommitmentsV1::default();
        let roots = [[0; 64]];
        let c = FlatSegContextV5 {
            profile: flat_trace_profile_id_v5(),
            claim: SegClaimContextV1 {
                program: &p,
                params: &pc,
                segment_roots: &roots,
                positions: 4,
                prompt_len: 4,
                prompt_root: prompt_root_of_ids_v1(&[1, 2, 3, 1]),
                inline_prompt: Some(&[1, 2, 3, 1]),
                generated: &[0],
                decode: crate::job::DecodeRuleV1::Greedy,
                encoder: None,
            },
        };
        let mut cap = limits(&e, 1);
        cap.history_tensor_bytes = 0;
        let mut session = FlatReplaySessionV6::new(e, cap);
        assert_eq!(
            session.check(&c, &base, &public, &|_, _| panic!("history cap precedes execution"), &[1, 2, 3, 1]),
            FlatReplayResultV6::Limit("history tensor bytes")
        );
        assert_eq!(public.calls.get(), 1);
        assert_eq!(session.attempts(), 0);
    }

    #[test]
    fn replay_v6_acquired_tensor_metadata_must_match_even_with_its_registered_root() {
        let mut t = Trial::of();
        for kind in 0..3 {
            let wrong = match kind {
                0 => Tensor::new(DType::I16, vec![257], vec![1; 257]).unwrap(),
                1 => Tensor::new(DType::I8, vec![129], vec![1; 129]).unwrap(),
                _ => {
                    let mut wrong = t.m.tensors[&(0, None)].clone();
                    wrong.data.push(1);
                    wrong
                }
            };
            if kind < 2 {
                t.pc.by_instance.insert((0, None), crate::merkle3::tensor_commitment_v3(&wrong));
            }
            let public = Public { trial: &t, missing: false, calls: Cell::new(0) };
            let mut session = FlatReplaySessionV6::new(t.envelope.clone(), limits(&t.envelope, 1));
            let result = session.check(&t.ctx(), &t.base, &public, &|_, _| Some(wrong.clone()), &[1]);
            let FlatReplayResultV6::Finding(FlatFindingV5::Inconsistent(why)) = result else {
                panic!("metadata substitution not refused: {result:?}")
            };
            assert!(why.contains("metadata differs from its program declaration"), "{why}");
            assert_eq!(public.calls.get(), 0);
            assert_eq!(session.attempts(), 1);
        }
    }
}
