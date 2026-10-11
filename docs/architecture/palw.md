# PALW — proof of AI work

**Design premise (above every ADR/RFC/spec):** [PRINCIPLES.md](../PRINCIPLES.md) — probabilistic detection, public localization,
deterministic adjudication, economic deterrence.

- **Governing ADRs:** [ADR-0173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md) (public verifier completeness), [ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md) (bond/time bounds), [ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md) (no model-availability consensus; read its D1 supplement), [ADR index](../adr/README.md).
- **RFCs 0001–0015:** [docs/rfc/](../rfc/) — inference surface, tensor IR and onboarding, generative classes, improvement, sharded Panels, certificates, claim-backed blocks, remote miner, permissionless Panel, long-context onboarding, PALW-only consensus, reproducible layouts, public prosecution, Panel-free verification.
- **Specs and design records:** [docs/spec/palw/](../spec/palw/), [docs/design/palw/](../design/palw/) (G14 completion matrix, readiness matrix, lane records).
- **2026-10-12 review and proposals:** [rfc-adr-review-resolution-2026-10-12.md](../design/palw/rfc-adr-review-resolution-2026-10-12.md), [panel1-bond-budget-model-capital-amendments-2026-10-12.md](../design/palw/panel1-bond-budget-model-capital-amendments-2026-10-12.md).
- **Code:** the PALW crates live under `misaka/palw/` (see the repository layout in the [README](../../README.md)); consensus integration under `consensus/core/src/palw_*`.
