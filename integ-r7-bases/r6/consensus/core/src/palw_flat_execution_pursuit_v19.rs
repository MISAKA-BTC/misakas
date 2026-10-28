//! Explicit original network clock assumptions for a complete public pursuit.
//! This does not assert measured speed, profitability or a detection floor. The
//! native contract binds the old OPV clocks without changing OPV registration.
use crate::palw_flat_execution_v11::*;
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::{
    flat_execution11::*,
    flat_quota11::*,
    hash::{Digest, object_id},
    ledger::LedgerPolicyV1,
    opv::OpvPolicyV1,
};
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativePublicPursuitV19 {
    pub version: u16,
    pub class: Digest,
    pub program: Digest,
    pub max_scope9: Digest,
    pub original_job_plan: Digest,
    pub opv_policy: Digest,
    pub ledger_policy: Digest,
    pub actual_positions: u32,
    pub nodes_per_position: u64,
    pub root_requests: u32,
    pub typed_requests: u32,
    pub source_requests: u32,
    pub requests: u32,
    pub filing_chunks: u32,
    pub first_step: u64,
    pub prove_before_seal: u64,
    pub turnaround: u64,
    pub answer_window: u64,
    pub complete_path: u64,
    pub final_ready: bool,
}
impl NativePublicPursuitV19 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/original-public-pursuit/v19", self)
    }
    pub fn latest_start(&self, horizon: u64) -> Option<u64> {
        horizon.checked_sub(self.complete_path)?.checked_sub(1)
    }
}
fn log(n: u64) -> u32 {
    if n <= 1 { 0 } else { 64 - (n - 1).leading_zeros() }
}
fn sum(items: &[u64]) -> Result<u64, String> {
    items.iter().try_fold(0u64, |a, n| a.checked_add(*n).ok_or("native public pursuit clock overflow".into()))
}
pub fn derive_native_public_pursuit_v19(
    class: &CheckedNativeExecutionClassV11,
    job: &CheckedJobQuotaPlanV11,
    p: &NativeExecutionPoliciesV11,
    ledger: &LedgerPolicyV1,
    opv: &OpvPolicyV1,
) -> Result<NativePublicPursuitV19, String> {
    let c = class.record();
    let j = job.record();
    if j.program_root != c.program_root
        || j.parameter_root != c.parameter_root
        || j.class_scope_root != c.class_scope9_root
        || j.positions == 0
        || j.positions > c.max_positions
        || ledger.network_domain != p.execution.network_domain
    {
        return Err("native public pursuit changes original class/job/network".into());
    }
    let view = class.view();
    let wiring = misaka_palw_kernel::trace::WiringV1::new(&view.program).map_err(|e| e.to_string())?;
    let mut nodes = 0u64;
    let mut replies = 0u32;
    let mut arity = 0u32;
    let mut supported = true;
    let mut topk_requests = 0u32;
    let ordinary: std::collections::BTreeMap<_, _> =
        view.scope.relations.iter().map(|r| ((r.occurrence, r.node), r.ordinary_point)).collect();
    for (s, (b, _)) in wiring.occurrences.iter().enumerate() {
        let h = wiring.h(s as u16, c.max_positions - 1);
        let block = &view.program.blocks[*b as usize];
        nodes = nodes.checked_add(block.nodes.len() as u64).ok_or("native node count overflow")?;
        for (n, node) in block.nodes.iter().enumerate() {
            arity = arity.max(node.inputs.len() as u32);
            let shape: Vec<_> = node.out.resolve(h).iter().map(|n| *n as u64).collect();
            let bounds = misaka_palw_kernel::merkle5::flat_material_bounds_v5(node.out.dtype, &shape)
                .ok_or("native public output layout bound")?;
            replies = replies.max(u32::try_from(bounds.localization_replies).map_err(|_| "native flat localization count")?);
            let is_ordinary = *ordinary.get(&(s as u16, n as u16)).ok_or("native original maxscope relation absent")?;
            if let misaka_palw_tir::Prim::TopK { k, .. } = node.prim {
                let dims = node.out.resolve(h);
                let bound = misaka_palw_kernel::flat_topk_locator21::topk_relation_callback_bound_v21(&dims, k)
                    .ok_or("native original TopK selected-row callback bound")?;
                topk_requests = topk_requests.max(u32::try_from(bound).map_err(|_| "native TopK callback count overflow")?);
            } else if !is_ordinary && !matches!(node.prim, misaka_palw_tir::prim::Prim::HistAppend { .. }) {
                supported = false;
            }
        }
    }
    if nodes == 0 {
        return Err("native public empty node directory".into());
    }
    let root_requests = log(j.positions.min(misaka_palw_kernel::seg::SEG_LEN_V4) as u64)
        .checked_add(log(nodes))
        .and_then(|n| n.checked_add(1))
        .ok_or("native root requests overflow")?;
    // Ordinary: output node, typed header, flat bisection, selected value, and
    // each prior public source node. Legacy history needs no more than this.
    // Own parameter/operand value proofs are authenticated against ORIGINAL PC
    // and prior public roots; they do not require public value callbacks.
    let typed_requests =
        replies.checked_add(3).and_then(|n| n.checked_add(arity)).ok_or("native typed request overflow")?.max(topk_requests);
    let mut pc_leaves = 1u64;
    for param in &view.program.params {
        let dims: Vec<_> = param.shape.iter().map(|n| *n as usize).collect();
        let layout = misaka_palw_kernel::merkle3::LayoutV3::try_of(&dims).ok_or("native source PC layout overflow")?;
        pc_leaves = pc_leaves
            .max(layout.leaves(misaka_palw_kernel::merkle::AXIS_ROW))
            .max(layout.leaves(misaka_palw_kernel::merkle::AXIS_COL));
    }
    let source_requests = log(pc_leaves).checked_add(3).ok_or("native PC locator count overflow")?;
    // Greedy decode after a clean full directory uses one public logits node
    // and at most two independently built own leaves, hence fits either path.
    let requests =
        root_requests.checked_add(typed_requests).ok_or("native complete request count overflow")?.max(source_requests).max(1);
    let filing = j
        .source_court_bound
        .max_proof_bytes
        .max(view.scope.worst_court_bytes)
        .checked_add(7 + 16 * 1024)
        .ok_or("native original filing extent overflow")?;
    let filing_chunks = u32::try_from(filing.div_ceil(32 << 10)).map_err(|_| "native original proof parts overflow")?;
    if filing_chunks == 0 || filing_chunks > 64 {
        return Err("native proof cannot fit owned protected chunk capacity".into());
    }
    let b = &opv.budgets;
    let first_step = sum(&[b.cold_material_daa, b.check_daa, b.carrier_daa, b.reorg_slack_daa])?;
    let prove_before_seal = sum(&[b.localize_daa, b.court_daa])?;
    let turnaround = sum(&[b.localize_daa, b.carrier_daa])?.max(1);
    let answer_window = c.horizons.da_answer;
    let repeated =
        sum(&[answer_window, turnaround])?.checked_mul(requests as u64).ok_or("native sequential request clock overflow")?;
    let transport = (filing_chunks as u64)
        .checked_mul(misaka_palw_kernel::budget::ROUTE_DAA_PER_CHUNK_V1)
        .ok_or("native protected carriage clock overflow")?;
    let complete_path = sum(&[
        first_step,
        c.horizons.seal_maturity,
        repeated,
        prove_before_seal,
        b.carrier_daa,
        b.reorg_slack_daa,
        c.horizons.seal_maturity,
        transport,
        b.carrier_daa,
        b.reorg_slack_daa,
    ])?;
    let seal_path = sum(&[c.horizons.seal_maturity, transport, b.carrier_daa, b.reorg_slack_daa])?;
    let response_compute = sum(&[b.disclose_daa, b.carrier_daa])?;
    let final_ready = supported
        && p.discovery.requests_per_session >= requests
        && c.horizons.challenge_window > complete_path
        && c.horizons.seal_ttl >= seal_path
        && answer_window >= response_compute;
    Ok(NativePublicPursuitV19 {
        version: 19,
        class: c.id(),
        program: c.program_root,
        max_scope9: c.class_scope9_root,
        original_job_plan: j.root(),
        opv_policy: object_id(b"misaka-palw/native/original-OPV-clock-policy/v19", opv),
        ledger_policy: object_id(b"misaka-palw/native/original-ledger-clock-policy/v19", ledger),
        actual_positions: j.positions,
        nodes_per_position: nodes,
        root_requests,
        typed_requests,
        source_requests,
        requests,
        filing_chunks,
        first_step,
        prove_before_seal,
        turnaround,
        answer_window,
        complete_path,
        final_ready,
    })
}
