# Documentation map

The code, current `main` CLI `--help` and ADR decisions are authoritative. This index separates live operator instructions from dated engineering evidence.

## Current Testnet-11 operator documents

- [Join as a PALW producer](testnet11-join-mining.md)
- [Run a full node](testnet11-node-operator.md)
- [The DAA 7,101 PALW upgrade and Verification V2 at 7,200](testnet11-7101-upgrade-announcement.md) — what fires at 7,100, 7,101, 7,200, 7,300, 7,301 and 6,900, and how to diagnose the execution lane
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
- fingerprint: `400403b8431082c9464d7326c3c11f77425ef3dbc41110f85a0dd28cb6f5f2d8` (the current main build; DAA 7,100 is the held/deep-audit boundary, 7,101 is the `6001`-named PALW upgrade bundle including ADR-0125's 1-BPS execution lane, 7,200 arms ADR-0133 Verification V2/readiness multiproofs, 7,300 shortens the execution span 5 DAA → 1 DAA, and 7,301 retires the compute overlay)
- PALW cadence: 120 seconds per block
- class shares at Relaunch 5f genesis: Floor 22‰, A16 489‰, QWEN36 489‰
- DNS Testnet-11 minimum stake Bond: 10 MSK
- ADR-0123 epoch-budget release: implemented, dormant on shipped presets
