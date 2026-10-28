//! Source-reading test support for production statements moved out of the former
//! panel parent. This scans physical modules; it does not reconstruct an old worker.
//! Tests of ordering across calls must check the orchestrator separately.
pub(crate) fn former_parent_production_source_v1() -> &'static str {
    static SOURCE: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    SOURCE.get_or_init(|| {
        let parent = include_str!("../palw_panel.rs");
        let end = parent.find("#[cfg(test)]\nmod tests").expect("the old parent production boundary");
        let mut parts = vec![&parent[..end]];
        for source in [
            include_str!("court_progress.rs"),
            include_str!("seat_progress.rs"),
            include_str!("collector.rs"),
            include_str!("retirement.rs"),
            include_str!("registration.rs"),
            include_str!("ingress.rs"),
            include_str!("startup.rs"),
            include_str!("tick_inputs.rs"),
            include_str!("material_pool.rs"),
        ] {
            parts.push(source.split("#[cfg(test)]").next().unwrap());
        }
        parts.join("\n")
    })
}
