# Mining (PALW producer)

MISAKA blocks are produced by **PALW** (proof of AI work): a producer bonds collateral, runs a registered model on a job, and the
chain pays a claim only after it reaches `Final` under the rules in force. Read the [design premise](../PRINCIPLES.md) first.

- **Join testnet-12 as a producer:** [testnet12-join-mining.md](../operations/testnet-12/testnet12-join-mining.md) — the current, maintained guide.
- **Launch note and finality:** [t12-launch-2026-09-25.md](../operations/testnet-12/t12-launch-2026-09-25.md) — never count blocks or DAA as finality on testnet-12.
- **Add or certify a model:** [palw-add-a-model-runbook.md](../operations/models/palw-add-a-model-runbook.md), [palw-certify-a-new-model.md](../operations/models/palw-certify-a-new-model.md), [palw-model-onboarding-sdk.md](../operations/models/palw-model-onboarding-sdk.md).
- **Remote (node-less) mining:** [RFC-0009](../rfc/0009-palw-remote-miner.md) and its [implementation record](../design/palw/rfc-0009-remote-record.md).
- **Bond and reward limits:** [ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md); model allocation [ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md).

Testnet-11 mining guides are kept in [history/](../history/README.md) and are not current instructions.
