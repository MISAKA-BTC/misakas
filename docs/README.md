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
- fingerprint: `ae1d61628da50c7becea62f0a8f08c8654d190c60b2e104df0010b121ba4d3d8`
- PALW cadence: 120 seconds per block
- class shares at Relaunch 5f genesis: Floor 22‰, A16 489‰, QWEN36 489‰
- DNS Testnet-11 minimum stake Bond: 10 MSK
- ADR-0123 epoch-budget release: implemented, dormant on shipped presets
