//! Generated matching public/private ledger visitor.
use super::*;
impl NativeHeapV1 for misaka_palw_kernel::ledger::KernelLedgerV1 {
    fn native_heap_v1(&self, path: &str) -> NativeCoverageV1 {
        let mut sum = self.native_inventory_hidden_v1().native_heap_v1(path);
        sum = sum.add::<Self>(self.flat_conformance_classes_v9.native_heap_v1(path), path);
        sum = sum.add::<Self>(self.flat_conformance_classes_v7.native_heap_v1(path), path);
        sum = sum.add::<Self>(self.policy.native_heap_v1(path), path);
        sum = sum.add::<Self>(self.schedule.native_heap_v1(path), path);
        sum = sum.add::<Self>(self.consumer_final_reward_caps.native_heap_v1(path), path);
        sum = sum.add::<Self>(self.consumer_minimum_payouts_v1.native_heap_v1(path), path);
        sum = sum.add::<Self>(self.known.native_heap_v1(path), path);
        sum = sum.add::<Self>(self.daa.native_heap_v1(path), path);
        sum = sum.add::<Self>(self.bonds.native_heap_v1(path), path);
        sum = sum.add::<Self>(self.classes.native_heap_v1(path), path);
        sum = sum.add::<Self>(self.conformance_classes.native_heap_v1(path), path);
        sum = sum.add::<Self>(self.pipeline_classes.native_heap_v1(path), path);
        sum = sum.add::<Self>(self.jobs.native_heap_v1(path), path);
        sum = sum.add::<Self>(self.pipeline_jobs.native_heap_v1(path), path);
        sum = sum.add::<Self>(self.claims.native_heap_v1(path), path);
        sum = sum.add::<Self>(self.demands.native_heap_v1(path), path);
        sum = sum.add::<Self>(self.served.native_heap_v1(path), path);
        sum = sum.add::<Self>(self.attested_artifacts.native_heap_v1(path), path);
        sum = sum.add::<Self>(self.job_claims.native_heap_v1(path), path);
        sum = sum.add::<Self>(self.seals.native_heap_v1(path), path);
        sum = sum.add::<Self>(self.proof_seals.native_heap_v1(path), path);
        sum = sum.add::<Self>(self.job_escrows.native_heap_v1(path), path);
        sum = sum.add::<Self>(self.exec_reward_holds.native_heap_v1(path), path);
        sum = sum.add::<Self>(self.beacon_sources.native_heap_v1(path), path);
        sum = sum.add::<Self>(self.beacon_source_inputs.native_heap_v1(path), path);
        sum = sum.add::<Self>(self.paid_claim_consumers_v2.native_heap_v1(path), path);
        sum = sum.add::<Self>(self.beacon_source_fee_inputs.native_heap_v1(path), path);
        sum = sum.add::<Self>(self.beacon_source_fees.native_heap_v1(path), path);
        sum = sum.add::<Self>(self.beacon_source_fee_promises.native_heap_v1(path), path);
        sum = sum.add::<Self>(self.served_demands.native_heap_v1(path), path);
        sum = sum.add::<Self>(self.claim_beacon_salts.native_heap_v1(path), path);
        sum = sum.add::<Self>(self.forfeited_claim_seals.native_heap_v1(path), path);
        sum = sum.add::<Self>(self.job_posters.native_heap_v1(path), path);
        sum = sum.add::<Self>(self.burned.native_heap_v1(path), path);
        sum = sum.add::<Self>(self.opv.native_heap_v1(path), path);
        sum = sum.add::<Self>(self.tiled_jobs.native_heap_v1(path), path);
        sum = sum.add::<Self>(self.seg_progress.native_heap_v1(path), path);
        sum = sum.add::<Self>(self.typed.native_heap_v1(path), path);
        sum = sum.add::<Self>(self.provider_liable.native_heap_v1(path), path);
        sum = sum.add::<Self>(self.court_default_held.native_heap_v1(path), path);
        sum = sum.add::<Self>(self.commitment_scope_from_daa.native_heap_v1(path), path);
        sum = sum.add::<Self>(self.verifier_pay_policy.native_heap_v1(path), path);
        sum = sum.add::<Self>(self.verifier_pay.native_heap_v1(path), path);
        sum = sum.add::<Self>(self.court_scope_policy.native_heap_v1(path), path);
        sum = sum.add::<Self>(self.court_scope.native_heap_v1(path), path);
        sum = sum.add::<Self>(self.sealed_bounty_policy.native_heap_v1(path), path);
        sum = sum.add::<Self>(self.claim_clock_from_daa.native_heap_v1(path), path);
        sum
    }
}
