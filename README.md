<h1>misakas — post-quantum (PQ-only) Kaspa with proof of AI work</h1>

> **MISAKAの設計前提(すべての ADR・RFC・Spec の上位): [不可侵原則](docs/PRINCIPLES.md)** — 確率的に検出し、公開証拠で局所化し、決定論的に裁き、経済的に不正を抑止する。 *Probabilistic Detection. Public Localization. Deterministic Adjudication. Economic Deterrence.*

**misakas** is a post-quantum, **PQ-only** fork of [rusty-kaspa](https://github.com/kaspanet/rusty-kaspa). It replaces Kaspa's
secp256k1/Schnorr transaction authorization with **ML-DSA-87** (FIPS 204, NIST category 5) and makes every non-PQ path
unrepresentable at the consensus, mempool and wallet layers. Blocks are produced by **PALW** (proof of AI work): bonded producers run
registered models, and a claim is paid only once it reaches `Final` under objective, publicly checkable rules. It is an independent
network with its own genesis and is not compatible with Kaspa or any prior kaspa-pq chain state.

The native token is **Misaka**, ticker **BILI** ([ADR-0174](docs/adr/0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)).
Addresses keep the `misaka` prefix (`misaka…` mainnet, `misakatest…` testnet, `misakadev…` devnet). The node binary is still named
`kaspad` and upstream crates keep their `kaspa-*` names.

## Current network: testnet-12

```bash
kaspad --testnet --netsuffix=12
```

The [published G14-only candidate](https://github.com/MISAKA-BTC/misakas/releases/tag/testnet12-g14-17400-655b6cf3a) is built from
`655b6cf3a8f61ebdce920af867f6bedd87e717d6`, with G14 scheduled at **DAA 17,400**. Its fingerprint is
**`73e67678a432effb4a982386d67383b95cf172babdde3e6f86c31cf3268d5bd5`**, genesis
`a27f8f44fe4d91a5…`, and schedule id `fd42c16706195ec0a6fa63902e1bd612303136b342bd29424e28be22599a5f2e`.
The Linux x86-64 binaries, SHA-256 sums and evidence are attached to that release.

This is the verified G14 candidate, not completion of the ongoing full-feature implementation. The latest recorded fleet state is the
old release after the IBM rollback; publication does not deploy it. See [build, evidence and deployment status](docs/design/palw/integration/g14-17000/release-17400-2026-10-28.md)
and [full-feature implementation status](docs/design/palw/integration/g14-17000/full-activation-17400-readiness-2026-10-28.md).
Read the [launch note](docs/history/networks/testnet-12/t12-launch-2026-09-25.md) before relying on the chain —
**never count blocks, blue-score depth or DAA as finality on testnet-12**; use finality depth or a `Final` PALW anchor.

New-genesis policy removes pre-registered genesis bonds. Participants wait for the intended public BASE recovery path and then fund
ordinary bonds from BASE earnings or genesis UTXOs; implementation and startup verification remain separate. See
[RFC-0041](docs/rfc/0041-genesis-bond-free-bootstrap.md).

## Quick start

| | |
| --- | --- |
| Build from source | [docs/getting-started/build.md](docs/getting-started/build.md) |
| Run a node | [docs/getting-started/run-node.md](docs/getting-started/run-node.md) |
| Mine (PALW producer) | [docs/getting-started/mining.md](docs/getting-started/mining.md) |
| Run a validator | [docs/getting-started/validator.md](docs/getting-started/validator.md) |
| Benchmarks and tests | [docs/getting-started/testing.md](docs/getting-started/testing.md) |

## Documentation

Start at the [documentation map](docs/README.md): architecture ([overview](docs/architecture/overview.md), [what is different from
Kaspa](docs/architecture/differences-from-kaspa.md)), the [ADRs](docs/adr/README.md), [RFCs 0001–0015](docs/rfc/), specs, design
records, operations and retained history. The previous, much longer README (testnet-11 era) is kept in
[docs/history/README-2026-10-12.md](docs/history/README-2026-10-12.md).

## Repository layout

Upstream rusty-kaspa components keep their places (`consensus/`, `crypto/`, `rpc/`, `protocol/`, `mining/`, `wallet/`, `database/`,
`kaspad/`, `bridge/` (Stratum bridge), …). MISAKA-specific code is grouped under `misaka/`, and package names do not change.

| Path | Contents |
| --- | --- |
| `misaka/palw/` | PALW crates: `core`, `kernel`, `base0`, `sdk`, `gateway`, `agent`, `worker`, `remote`, `panel`, `security`, … and `tir/{core,exec,lower,artifact,conformance,sketch}` (package names stay `misaka-palw-*`) |
| `testing/reference/` | Independent second implementations (`palw-reference2`, `palw-tir-ref2`, `palw-base0-ref2`) |
| `experiments/` | Isolated prototypes with their own workspace (`palw-tir-gpu`) |
| `misaka/pq/` | Post-quantum `signer`, `validator`, `validator-core` and `miner` (packages `kaspa-pq-*`) |
| `misaka/evm/` | EVM lane: `core` (package `kaspa-evm`), `indexer/{core,store-pg,service}`, Solidity `contracts` |
| `misaka/apps/`, `misaka/endpoints/` | MISAKA applications (`cli` → binary `misaka`, `misaminer`, `dnsseeder`) and the endpoint list |
| `tools/drills/` | Drill harnesses (combined, gen, improve, tir, vertex) and `tools/` utilities |
| [`scripts/`](scripts/README.md) | `ci-gates.sh` plus `ci/`, `release/`, `verify/`, `drills/`, `ops/`, `sim/` |
| `docs/` | Documentation |

## Upstream and license

Forked from [kaspanet/rusty-kaspa](https://github.com/kaspanet/rusty-kaspa); see [LICENSE](LICENSE), [CONTRIBUTING.md](CONTRIBUTING.md)
and [SECURITY.md](SECURITY.md).
