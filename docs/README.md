# Documentation map

The code, current `main` CLI `--help` and ADR decisions are authoritative. This index separates live operator instructions from dated engineering evidence.

## Current Testnet-11 operator documents

- [Join as a PALW producer](testnet11-join-mining.md)
- [Run a full node](testnet11-node-operator.md)
- [The DAA 7,101 PALW upgrade and Verification V2 at 7,200](testnet11-7101-upgrade-announcement.md) — what fires at 7,100, 6,001, 6,100, 6,201 and 6,900, and what an operator must do before 6,000
- [Pre-arming security audit of the DAA 7,101 bundle (2026-09-18)](palw-audit-2026-09-18-6001.md) — two Critical and six High findings, the seven release blockers and their fixes, and what is left as residual risk
- [The DAA clock under the 7,101 bundle (2026-09-18)](palw-daa-clock-audit-2026-09-18.md) — every lane's effect on the DAA score, `bits` and blue work; the window arithmetic at 120 s / measured / max lane load; the blocker and its fix (ADR-0138)
- [Run a DNS-finality validator](validator-runbook.md)
- [Operate model classes](palw-public-testnet-classes-runbook.md)
- [Add a model through the SDK](palw-model-onboarding-sdk.md)
- [Free-prompt mining](testnet11-free-prompt-mining.md)
- [EVM flat backend migration](misaka-evm-flat-backend-runbook-v0.1.md)
- [Node liveness probe](node-liveness-probe.md)

## Governing design

- [ADR index](adr/README.md)
- [PQ specification](kaspa-pq-spec.md)
- [ML-DSA-87 design](kaspa-pq-design-mldsa87.md)
- [PALW registry map](palw-registry-map.md)
- [PALW extension envelope](palw-extension-envelope.md)

## Historical evidence

Files whose names contain a date, audit, launch record, old relaunch, testnet-10, testnet-12, testnet-21, shadow drill or transition are retained as engineering evidence. Their measured hashes, peers, DAA scores, class IDs and commands describe that historical run and are not current operator defaults.

In particular, do not derive a current command from:

- `testnet10-*.md`
- `testnet11-relaunch2-*.md`, `relaunch5*.md` or old genesis cards
- dated `palw-*-2026-*.md` audits and measurements
- archived mainnet-readiness or drill reports

When a historical report conflicts with a current operator document, use the current operator document and verify against `--help` and chain RPC.

## Current invariants worth checking

- network: `testnet-11`
- fingerprint: `c3a5e91dfc9336b02d2280ccb10327e19058123b8589da2d0aa0754f719e9a5f` (schedules the DAA 7,101 flag day: ADR-0124, 0125, 0126, 0128, 0130 M1/M2, ADR-0135's permissionless model registry, ADR-0132 Upgrade C's economic payout, ADR-0137's work target, ADR-0132 S's single lottery and §7.6's short challenge window — one PALW upgrade day, pinned by `t11_daa_6000_is_the_compatibility_boundary_and_6001_the_one_palw_upgrade_flag_day` — ADR-0133's Verification V2 (S1) at 7,200, and ADR-0134's retirement at 7,301)
- PALW cadence: 120 seconds per block
- class shares at Relaunch 5f genesis: Floor 22‰, A16 489‰, QWEN36 489‰
- DNS Testnet-11 minimum stake Bond: 10 MSK
- ADR-0123 epoch-budget release: implemented, dormant on shipped presets
