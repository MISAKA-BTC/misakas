# Operations

Current, maintained operator documents. Dated audits and old-network runbooks are kept for evidence in [audit/](../audit/) and [history/](../history/README.md).

| Task | Document |
| --- | --- |
| Join testnet-12 as a producer | [testnet12-join-mining.md](testnet-12/testnet12-join-mining.md) |
| testnet-12 launch note and finality | [t12-launch-2026-09-25.md](testnet-12/t12-launch-2026-09-25.md), [t12-rcore-launch-checklist.md](testnet-12/t12-rcore-launch-checklist.md) |
| Run a DNS-finality validator | [validator-runbook.md](validator/validator-runbook.md) |
| Operate model classes | [palw-public-testnet-classes-runbook.md](models/palw-public-testnet-classes-runbook.md) |
| Add / certify a model | [palw-add-a-model-runbook.md](models/palw-add-a-model-runbook.md), [palw-certify-a-new-model.md](models/palw-certify-a-new-model.md), [palw-model-onboarding-sdk.md](models/palw-model-onboarding-sdk.md) |
| Node liveness probe | [node-liveness-probe.md](tools/node-liveness-probe.md) |
| EVM flat backend | [misaka-evm-flat-backend-runbook-v0.1.md](evm/misaka-evm-flat-backend-runbook-v0.1.md) |
| PQ verification | [kaspa-pq-mldsa87-verification-runbook.md](pq/kaspa-pq-mldsa87-verification-runbook.md) |
| Mainnet readiness / certification | [mainnet-readiness.md](mainnet/mainnet-readiness.md), [mainnet-palw-certification-runbook.md](mainnet/mainnet-palw-certification-runbook.md) |
| Release process | [release-process.md](release-process.md) |

## Layout

| Directory | Contents |
| --- | --- |
| [testnet-12/](testnet-12/) | join as a producer, launch note, R-core launch checklist |
| [models/](models/) | add / certify a model, SDK, adjudicability guide, classes runbook, step-space notes, extension envelope, model requests |
| [validator/](validator/) | DNS-finality validator runbook |
| [evm/](evm/) | EVM flat backend runbook, connecting Ethereum tooling |
| [pq/](pq/) | ML-DSA-87 verification runbook |
| [mainnet/](mainnet/) | mainnet readiness and PALW certification runbook |
| [tools/](tools/) | node liveness probe, misaka-probe and miner-address-check notes |
| `release-process.md`, `override-params.md` | release process; `--override-params-file` |

Where each file came from: [../history/MOVED.md](../history/MOVED.md).

