# Documentation map

The code, current `main` CLI `--help` and ADR decisions are authoritative. This index separates live operator instructions from dated engineering evidence.

## Current Testnet-11 operator documents

- [Join as a PALW producer](testnet11-join-mining.md)
- [Run a full node](testnet11-node-operator.md)
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
- fingerprint: `dd805c9f2c4e9db3c0d6ffa2d87fa6ffb4263078ab8b7eb857f8fb11f8aa010c` (schedules the DAA 7,001 flag day: ADR-0124, 0125, 0126, 0128, 0130 M1/M2 — and ADR-0134's retirement at 7,201)
- PALW cadence: 120 seconds per block
- class shares at Relaunch 5f genesis: Floor 22‰, A16 489‰, QWEN36 489‰
- DNS Testnet-11 minimum stake Bond: 10 MSK
- ADR-0123 epoch-budget release: implemented, dormant on shipped presets
