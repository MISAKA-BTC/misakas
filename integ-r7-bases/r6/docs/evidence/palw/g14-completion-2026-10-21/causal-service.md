# Causal service reservation component — 2026-10-21

The complete original admission coordinator must derive all legal packets, authenticated actors, dependencies, actual resource costs, capital and fees. This change implements only deterministic causal scheduling of that graph; caller input does not certify completeness or funding.

A plain earliest-deadline reservation can place a response before its prerequisite request. The regression constructs that counterexample; the causal planner refuses it without changing the book. It retains every sibling, orders ready nodes canonically, waits until the slot after every predecessor, and never moves existing reservations. Missing/duplicate dependencies, cycles and exhausted chain deadlines fail atomically. Bounds and original hard deadlines are unchanged.

The caller must retain or independently rederive the graph after reload and repair. `check_causal_v1` checks the projection, but existing v2 generic repair is not a causal graph recovery implementation. Scheduled quota is neither delivered material nor an eligible duty/default verdict. No production original route, fee source, supply decision, public pursuit or activation is introduced.

## Verification

`misaka-heavy-lock.sh cargo test --release -p kaspa-consensus-core --lib palw_court_service_v1 -- --test-threads=1`: **4 passed / 0 failed**, 4703 filtered. Raw output: [causal-service-tests.log](causal-service-tests.log). Exact two changed Rust source hashes and preserved executable: [causal-service-build.json](causal-service-build.json). These are component tests, not node or throughput measurements and not whole-G14 acceptance.
