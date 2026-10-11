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

Verify the consensus params fingerprint **`5ee7fd8ee019968cf52929b844cf9ddfb1aad500842a89cf04bced8ba4edefb6`**, genesis
`a27f8f44fe4d91a5…` and schedule id `1678e07359f6727e…`. Read the [launch note](docs/t12-launch-2026-09-25.md) before relying on the
chain — **never count blocks, blue-score depth or DAA as finality on testnet-12**; use finality depth or a `Final` PALW anchor.
New consensus rules wait, dormant, for a single full-activation release ([readiness matrix](docs/design/palw/activation-readiness-matrix.md)).

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
`kaspad/`, …). MISAKA-specific code is grouped as follows.

| Path | Contents |
| --- | --- |
| `misaka-palw*/` | PALW crates (kernel, SDK, TIR, worker, gateway, remote miner, Panel, …) |
| `kaspa-pq-*`, `pq-miner/` | Post-quantum signer, validator and miner |
| `kaspa-evm/`, `evm-indexer/`, `bridge/`, `contracts/` | EVM lane |
| `misaka-cli/`, `misaminer/`, `misaka-dnsseeder/`, `misaka-endpoints/` | MISAKA applications |
| `tools/drills/` | Drill harnesses (combined, gen, improve, tir, vertex) and `tools/` utilities |
| [`scripts/`](scripts/README.md) | `ci-gates.sh` plus `ci/`, `release/`, `verify/`, `drills/`, `ops/`, `sim/` |
| `docs/` | Documentation |

## Upstream and license

Forked from [kaspanet/rusty-kaspa](https://github.com/kaspanet/rusty-kaspa); see [LICENSE](LICENSE), [CONTRIBUTING.md](CONTRIBUTING.md)
and [SECURITY.md](SECURITY.md).
