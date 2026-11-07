pub mod palw_source_guard_v1 {
pub const PALW_PROCESSOR_SOURCE_FILES_V1: &[&str] = &[
    "processor.rs",
    "processor/palw_validate.rs",
    "processor/palw_validate/panel.rs",
    "processor/palw_validate/execution.rs",
    "processor/palw_validate/reporting.rs",
    "processor/palw_validate/court.rs",
    "processor/palw_validate/model_market.rs",
    "processor/palw_validate/class_registration.rs",
    "processor/palw_validate/bond_readiness.rs",
    "processor/palw_validate/kernel_material.rs",
    "processor/palw_validate/improvement.rs",
    "processor/palw_validate/structural.rs",
    "processor/palw_accept.rs",
    "processor/palw.rs",
    "processor/evm.rs",
    "processor/dns.rs",
    "processor/template.rs",
];
pub fn palw_processor_source_v1() -> &'static str {
    static TEXT: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    TEXT.get_or_init(|| {
        let dir = std::path::Path::new("/Users/wata/.codex/worktrees/rfc14-g14-integration-candidate/MISAKA-testnet/consensus/core").join("../src/pipeline/virtual_processor");
        PALW_PROCESSOR_SOURCE_FILES_V1
            .iter()
            .map(|file| std::fs::read_to_string(dir.join(file)).unwrap_or_else(|e| panic!("{file}: {e}")))
            .collect::<Vec<_>>()
            .join("\n")
    })
}
}
#[test]
fn t12_030_the_processors_attribution_calls_are_the_shared_gates() {
        let processor = crate::palw_source_guard_v1::palw_processor_source_v1();
        let arm = &processor[processor
            .find("kaspa_consensus_core::palw_class_admission_v2::verify_class_admission_v9(\n                        bundle,")
            .expect("the ClassRegistered arm's gate")..];
        let arm = &arm[..arm.find("Obj::ClassFrozen { .. }").expect("the next class-registration arm")];
        let held = arm
            .find(
                "kaspa_consensus_core::palw_class_admission_v2::palw_held_class_is_attributable_v2(\n                        \
                 &carriage.profile,\n                        self.palw_offence_attribution_at(point.daa_score),\n                        self.palw_memory_routing_court_v2_at(point.daa_score),\n                    )",
            )
            .expect("the held check, with the fence at the block");
        let fence = arm.find("if self.palw_offence_attribution_at(point.daa_score) {").expect("the attribution check is fenced");
        let class = arm
            .find(
                "kaspa_consensus_core::palw_attempt_rules_v1::palw_attributable_class_v1(\n                            \
                 &carriage.profile,\n                            &carriage.canonical,\n                            false,\n                            \
                 self.palw_prompt_ids_form_at(point.daa_score),\n                        )",
            )
            .expect("the attribution check, with is_base false and the form at the block");
        assert!(held < fence && fence < class, "held first, then the fenced attribution check");
        assert_eq!(arm.matches("palw_attributable_class_v1(").count(), 1, "one call");
        assert_eq!(arm.matches("palw_held_class_is_attributable_v2(").count(), 1, "one call");
        // And the shared gate calls the same two, in the same order, with the same arguments.
        let this = include_str!("/Users/wata/.codex/worktrees/rfc14-g14-integration-candidate/MISAKA-testnet/consensus/core/src/palw_attempt_rules_v1.rs");
        let gate = &this[this.find("pub fn palw_registration_attribution_v2(").expect("the gate")..];
        let gate = &gate[..gate.find("\n}\n").expect("its end")];
        let g_held = gate
            .find("crate::palw_class_admission_v2::palw_held_class_is_attributable_v2(profile, offence_attribution, memory_routing_court)")
            .expect("the held check");
        let g_fence = gate.find("if offence_attribution {").expect("fenced");
        let g_class = gate.find("palw_attributable_class_v1(profile, canonical, false, network_form)").expect("the attribution check");
        assert!(g_held < g_fence && g_fence < g_class);
        // And every preflight reads the fence and the form from the params at the height the shape
        // is read at — the accessors the processor's are mirrors of.
        let admission = include_str!("/Users/wata/.codex/worktrees/rfc14-g14-integration-candidate/MISAKA-testnet/consensus/core/src/palw_class_admission_v2.rs");
        assert!(admission.contains("offence_attribution: params.palw_offence_attribution_active_at(daa_score),"));
        assert!(admission.contains("prompt_ids_form: params.palw_prompt_ids_form_at(daa_score),"));
        assert!(admission.contains("memory_routing_court: params.palw_memory_routing_court_v2_fence().is_some_and(|f| f.is_active(daa_score)),"));
    }
