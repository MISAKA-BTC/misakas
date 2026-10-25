# Mining (PALW producer)

MISAKA blocks are produced by **PALW** (proof of AI work): a producer bonds collateral, runs a registered model on a job, and the
chain pays a claim only after it reaches `Final` under the rules in force. Read the [design premise](../PRINCIPLES.md) first.

- **Join testnet-12 as a producer:** [testnet12-join-mining.md](../testnet12-join-mining.md) — the current, maintained guide.
- **Launch note and finality:** [t12-launch-2026-09-25.md](../t12-launch-2026-09-25.md) — never count blocks or DAA as finality on testnet-12.
- **Add or certify a model:** [palw-add-a-model-runbook.md](../palw-add-a-model-runbook.md), [palw-certify-a-new-model.md](../palw-certify-a-new-model.md), [palw-model-onboarding-sdk.md](../palw-model-onboarding-sdk.md).
- **Mine testnet-12 with no local node:** [testnet12-node-free.md](../testnet12-node-free.md). Connect to `wss://wallet.misakascan.com/t12/wrpc/borsh` (`169.58.232.113`) and `wss://llm.misakascan.com/t12/wrpc/borsh` (`169.58.39.220`). Do not open P2P `26311` for this path.
- **Remote miner design:** [RFC-0009](../rfc/0009-palw-remote-miner.md) and its [implementation record](../design/palw/rfc-0009-remote-record.md).
- **Bond and reward limits:** [ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md); model allocation [ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md).

Testnet-11 mining guides are kept in [history/](../history/README.md) and are not current instructions.
