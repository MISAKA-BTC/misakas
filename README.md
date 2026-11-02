<h1>misakas — post-quantum (PQ-only) Kaspa with proof of AI work</h1>

> **MISAKAの設計前提(すべての ADR・RFC・Spec の上位): [不可侵原則](docs/PRINCIPLES.md)** — 確率的に検出し、公開証拠で局所化し、決定論的に裁き、経済的に不正を抑止する。 *Probabilistic Detection. Public Localization. Deterministic Adjudication. Economic Deterrence.*

**misakas** is a post-quantum, **PQ-only** fork of [rusty-kaspa](https://github.com/kaspanet/rusty-kaspa). It replaces Kaspa's
secp256k1/Schnorr transaction authorization with **ML-DSA-87** (FIPS 204, NIST category 5) and makes every non-PQ path
unrepresentable at the consensus, mempool and wallet layers. Blocks are produced by **PALW** (proof of AI work): bonded producers run
registered models. At the G14 fence, payment requires both `Final` and claim-level `SecurityCertified` approval
from selected validators under the stated honest-fraction assumption; local replay alone does not authorize credit. It is an independent
network with its own genesis and is not compatible with Kaspa or any prior kaspa-pq chain state.

The native token is **Misaka**, ticker **BILI** ([ADR-0174](docs/adr/0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)).
Addresses keep the `misaka` prefix (`misaka…` mainnet, `misakatest…` testnet, `misakadev…` devnet). The node binary is still named
`kaspad` and upstream crates keep their `kaspa-*` names.

## Current network: testnet-12

```bash
kaspad --testnet --netsuffix=12
```

The full-feature implementation is being integrated and is **not yet complete**. Completion requires executable normal paths, a coherent whole-candidate build, and the required tests. See the [implementation plan](docs/design/palw/integration/g14-17000/full-implementation-completion-2026-10-31.md) and [historical readiness record](docs/design/palw/integration/g14-17000/full-activation-17400-readiness-2026-10-28.md).

The [published G14-only candidate](https://github.com/MISAKA-BTC/misakas/releases/tag/testnet12-g14-17400-655b6cf3a) is a separate, older artifact. The earlier G14 schedule is DAA **18,400**; full activation remains `None`. This working branch does not claim a new public deployment or advertise a verified full-candidate fingerprint.

The upstream all-model G14 Panel=1 candidate schedules its separate migration at **DAA 18,640**. Its recorded fingerprint is
**`edf472294c89dab0e64e625dbdfcd039b91081b05f89eacf8ba2aa25baa3a189`**, genesis
`a27f8f44fe4d91a5…`, and schedule id `4024a6e74d7aeb170d99e67c6d8fbe7884d71a716ccb778cd1ee526d01781b04`.

The upstream candidate is under Linux build and deployment validation. Code and local tests do not establish live activation,
Final or SecurityCertified. See [the migration and validation record](docs/evidence/palw/model-panel-one-2026-10-31/README.md),
[RFC-0042 (AllModelG14PanelOne)](docs/rfc/0042-all-model-g14-panel-one.md) and the [retained earlier G14 release record](docs/design/palw/integration/g14-17000/release-17400-2026-10-28.md).
These upstream candidate identities are retained history, not a verified fingerprint of this merged full-feature working tree. The separately adopted [RFC-0042 (CourtSupplierConsent)](docs/rfc/0042-court-family-supplier-consent-and-native-reservation.md) retains its own content and status. Both original RFC-0042 / ADR-0210 allocations are preserved by full path; see the [parallel-allocation record](docs/maintenance/numbering.md#2026-11-02-並行割当衝突).

Read the [launch note](docs/history/networks/testnet-12/t12-launch-2026-09-25.md) before relying on the chain —
**never count blocks, blue-score depth or DAA as finality on testnet-12**; use finality depth or a `Final` PALW anchor. Claim-level credit authorization remains a separate requirement.

The adopted zero-genesis-bond startup path is described in [RFC-0041](docs/rfc/0041-genesis-bond-free-bootstrap.md); its final same-candidate validation is part of the implementation goal.

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
