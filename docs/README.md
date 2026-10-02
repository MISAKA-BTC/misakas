# Documentation map

The code, the current `main` CLI `--help`, `release.json` and the ADR decisions are authoritative. When a document disagrees with them, the document is wrong.

The current network is **testnet-12**. Its identity (network, consensus fingerprint, schedule id, genesis) is in [`release.json`](../release.json); documents link there instead of repeating it.

## Start here

- [Architecture overview](architecture/overview.md) — the protocol as it is now, topic by topic, with the ADRs that govern each part and where its code lives
- [Mainnet readiness](mainnet-readiness.md) — what is done, partly done and not done before a mainnet genesis (its PASS / PARTIAL / TODO marks feed the CI summary)
- [Release process](release-process.md) — how a release is cut, signed and verified

## Operating on testnet-12

- [Join as a PALW producer](testnet12-join-mining.md) — the canonical operator guide: build, keys, node, funds, bond, start, observe, stop
- [Run a node](node-operator.md) — roles and where each is documented
- [Run a DNS-finality validator](validator-runbook.md)
- [Operate model classes](palw-classes-runbook.md)
- [Free-prompt gateway](palw-freeprompt-gateway.md) — run your own LLM and mine with the same inference
- [Ask the network for a file](ask-for-a-file.md)
- [Node liveness probe](node-liveness-probe.md), [`--override-params-file`](override-params.md), [archive nodes](archival.md)
- [EVM flat backend](misaka-evm-flat-backend-runbook-v0.1.md)
- Japanese operator memos: [misaka-probe](misaka-probe-usage-ja.md), [miner address check](misaka-miner-address-check-ja.md)
- [Wiki pages](wiki/Home.md) (mirrored to the GitHub wiki) — quick start, verification participation (JA), operations notes, FAQ

## testnet-12 records

- [Launch note (2026-09-25)](t12-launch-2026-09-25.md) — the release, its known issues and when a payment is final
- [Regenesis record (2026-09-23)](testnet-12-regenesis-2026-09-23.md)
- [R-core+ launch checklist](t12-rcore-launch-checklist.md)
- [Lane F1 panel seed (2026-09-25)](t12-panel-seed-2026-09-25.md)
- [Capacity stages (ADR-0160)](capacity/) — stage status and the flag-day integration notes

## Models

- [Requesting a model](model-requests.md) — what happens to a request, step by step
- [Model-onboarding SDK](palw-model-onboarding-sdk.md) and [converting an existing model to `.palwart` (JA)](palw-add-a-model-runbook.md)
- [Certifying a new model](palw-certify-a-new-model.md) and [mainnet certification objects](mainnet-palw-certification-runbook.md)
- [Model adjudicability guide (JA)](misaka-palw-model-adjudicability-guide-v0.1-ja.md)
- [Registry map](palw-registry-map.md), [extension envelope](palw-extension-envelope.md) (example manifests in [`extension-manifests/`](extension-manifests/)), [derived artifacts](palw-derived-artifacts.md), [components manifest](components-manifest.md)
- Held-context artifacts: [Qwen2.5 A16 2M](qwen25-a16-2m-held-artifact.md), [Qwen3.6 2M](qwen36-2m-held-artifact.md)
- Engineering notes: [court round-trip drill](palw-court-round-trip-drill.md), [BASE-0 PTQ pipeline scope](palw-base0-ptq-pipeline-scope.md), [threat model and red-test register](palw-rc-threat-model.md)

## EVM and Ethereum tooling

- [Connecting Ethereum tooling](connecting-ethereum-tooling.md)
- [Differences from Ethereum](evm-differences-from-ethereum.md) — the compat profile
- [JSON-RPC compatibility matrix](ethereum-rpc-compat-matrix.md)
- [Wallet profile `misaka-evm-hd-v1`](misaka-evm-wallet-profile-v1.md)
- [OpenAI-compatible surface](openai-surface/v1/README.md)

## Governing design

- [ADR index](adr/README.md) — the decisions; [RFCs](rfc/README.md) — proposals not yet decided
- [PQ specification](kaspa-pq-spec.md) and [test plan](test-plan-kaspa-pq.md)
- [Tensor IR spec](spec/palw/04b-tensor-ir.md)
- [`design/`](design/) — design documents (ML-DSA-87, EVM, PALW PoW/OTA/slash, PQ deposit, TIR). Each records the design at the version in its name; where it differs from an ADR or the code, the ADR and the code win.

## History

- [`archive/`](archive/README.md) — dated audits, measurements, drills, superseded plans and completed migrations. Values in them are not current.
- [`evidence/`](evidence/), [`testing/`](testing/), [`transcription-sources/`](transcription-sources/), [`explorer/`](explorer/README.md) — supporting material cited by ADRs and code.
- testnet-10 and testnet-11 documents were removed; they remain in git history.
