# Mainnet readiness

**Where MISAKA stands against what a production L1 needs before its mainnet genesis.** Each item is
marked only by what this repository can show. The mark is not a plan or an intention:

| mark | meaning |
|---|---|
| **PASS** | done, and the evidence is in the tree or in CI |
| **PARTIAL** | some of it is done; the rest is named on the line |
| **TODO** | not done yet |

Last reviewed: **2026-09-25**, against `main` at testnet-12's release commit `0e8ec984e`. A PASS
turns back into PARTIAL or TODO when later work removes its evidence. Anybody changing a mark
changes it in the same commit as the evidence. Every CI run counts the marks in this file into its
job summary (`scripts/misaka-ci-summary.py`), next to the test counts of that run.

**Summary.** The engineering base is strong: gated CI that can be reproduced locally, a pinned
toolchain, post-quantum isolation enforced by the build, many audits published with their fixes,
and an activation-fence upgrade path that has run on a live network. What keeps MISAKA from being a
mainnet candidate is **how fast the rules still change**. testnet-12 launched with two known
CRITICAL issues whose fixes arrive as post-launch fences, and the release has no tag, no
signatures and no SBOM. The next phase is to freeze the rules and prove they hold. It is not to
add more rules.

---

## 1. Consensus

| status | item | evidence / what is missing |
|---|---|---|
| **PASS** | Consensus tests run on every push | `Test Suite` job in [`.github/workflows/ci.yaml`](../.github/workflows/ci.yaml) (`cargo nextest run`, doctests, devnet-prealloc); `Context vectors (release)` runs the ADR-0110 4,096- and 32,768-position vectors in release mode |
| **PASS** | Consensus identity is explicit | every node prints its consensus params fingerprint and fence schedule at startup, and the handshake refuses a peer on a different ruleset (`consensus_params_id`). The release workflow starts every platform's binary and fails unless it prints the identity [`release.json`](../release.json) declares |
| **PASS** | Activation-fence upgrade path exercised live | testnet-11 crossed fences at DAA 1150, 1900, 2150, 2400, 3500, 4000, 7100 and more on a public network ([history](history/testnet-11.md)) |
| **PASS** | Genesis constants guarded | a change to the premine constants (`consensus/core/src/config/premine.rs`) moves every network's genesis hash and is refused at startup until re-pinned (audit M-07) |
| **PARTIAL** | Pinned fingerprints in tests | `shipped_presets_have_pinned_fingerprints` exists, but `scripts/ci-gates.sh --list` declares it a known red until the next single re-pin |
| **PARTIAL** | Launch gate (ADR-0152 §8.3) | [t12-rcore-launch-checklist.md](t12-rcore-launch-checklist.md) §1.0 recorded the gate as not met on 2026-09-25; testnet-12 launched with the gaps listed in [the launch note §2](t12-launch-2026-09-25.md) |
| **TODO** | **Consensus rules frozen** | testnet-12's two CRITICAL fixes (heartbeat transparency, panel-seed grinding) land as post-launch fences, and more are scheduled (launch note §2.5–2.7) |
| **TODO** | Mainnet genesis and parameters final | the `mainnet` parameter set is defined, but it is not declared final and no mainnet card is minted ([palw-mainnet-audit-2026-09-06.md](palw-mainnet-audit-2026-09-06.md) lists what must be true first) |
| **TODO** | The running ruleset's design is on record | testnet-12 runs R-core+ (ADR-0152 v3.1), which is cited throughout the code but not committed to `docs/adr/`; the [architecture overview](architecture/overview.md) is the current map until it is |
| **TODO** | Consensus differential testing | no second implementation or reference model is run against the node's consensus in CI |

## 2. Security

| status | item | evidence / what is missing |
|---|---|---|
| **PASS** | Post-quantum isolation enforced | `scripts/pq-ci-guard.sh` fails CI if `kaspa-consensus` or `kaspad` links secp256k1 |
| **PASS** | Dependency advisories gated | `cargo-deny` runs in the `pq-guard` gate ([`deny.toml`](../deny.toml)) |
| **PASS** | Internal audits, fixed in the open | e.g. [palw-audit-2026-09-18-6001.md](palw-audit-2026-09-18-6001.md) (2 Critical / 6 High, all fixed), [palw-daa-clock-audit-2026-09-18.md](palw-daa-clock-audit-2026-09-18.md), the mainnet audits of [08-28](palw-mainnet-audit-2026-08-28.md), [08-30](palw-mainnet-audit-2026-08-30.md), [09-05](palw-mainnet-audit-2026-09-05.md) and [09-06](palw-mainnet-audit-2026-09-06.md), and the release report [palw-release-6001-verdict-2026-09-18.md](palw-release-6001-verdict-2026-09-18.md) |
| **PARTIAL** | External review | commissioned static reviews of code snapshots: the 2026-06-22 Kaspa-diff and EVM/NFT packages (answered in [security/MISAKA-Audit-Remediation-Response-2026-06-23.md](security/MISAKA-Audit-Remediation-Response-2026-06-23.md)) and the 2026-08-21 PALW review ([palw-external-audit-2026-08-21.md](palw-external-audit-2026-08-21.md), which could not build or run the code). None of them covers a release. The reviewers are not named in the tree |
| **TODO** | Independent audit of a release candidate | a named third party, a tagged commit, and a published report for at least one of: consensus, cryptography, PALW economics and security, network and RPC attack surface |
| **PARTIAL** | Fuzzing | only the upstream arithmetic harnesses (`math/fuzz`, `crypto/muhash/fuzz`), not run in CI. Nothing fuzzes block, transaction, P2P message or RPC decoding |
| **PARTIAL** | Threat model | [palw-rc-threat-model.md](palw-rc-threat-model.md) covers PALW; there is none for the network and RPC surface |
| **PARTIAL** | Vulnerability disclosure | [SECURITY.md](../SECURITY.md) asks for private reports but names no contact address, key or response time |

## 3. Operations

| status | item | evidence / what is missing |
|---|---|---|
| **PASS** | Node, CLI and validator runbooks | [testnet12-join-mining.md](testnet12-join-mining.md), [validator-runbook.md](validator-runbook.md), `misaka node doctor` |
| **PARTIAL** | Recovery from real incidents | testnet-11 recovered from a partition, stale-build arms and re-mints ([history](history/testnet-11.md), [testnet11-regenesis-2026-08-30.md](testnet11-regenesis-2026-08-30.md)). Recovery was done by hand; no automated test proves that the same failure cannot recur |
| **PARTIAL** | Drills | [palw-court-round-trip-drill.md](palw-court-round-trip-drill.md), [palw-economy-studio-drill-2026-09-20.md](palw-economy-studio-drill-2026-09-20.md). The testnet-12 drill kit ([contrib/t12-drill-kit](../contrib/t12-drill-kit/README.md), D-1…D-10) is ready but has not run yet |
| **TODO** | Recovery drills run and published | fresh sync from genesis, crash and restart mid-IBD, DB corruption, network partition and heal, validator loss, seeder loss. Each should run as a CI job or scheduled job, and its result should be published |
| **TODO** | Large-scale simulation | `simpa` exists, but no published multi-node (e.g. 1,000-node) PALW simulation result |

## 4. Network

| status | item | evidence / what is missing |
|---|---|---|
| **PASS** | Public DNS seeders | `seeder1…4.misakascan.com` |
| **PASS** | Explorer, web wallet, public RPC | [misakascan.com](https://misakascan.com), [wallet.misakascan.com](https://wallet.misakascan.com), `misakascan.com/evm` |
| **PASS** | App on the chain | [misakaoptions.com](https://misakaoptions.com) ([web/misaka-options](../web/misaka-options/README.md)) |
| **PARTIAL** | Model market in use | active on testnet-12 from genesis and reachable from the app, the EVM lane and the CLI; at launch no store was opened yet, and two of the three genesis classes were still `Prefetching` (`getPalwModelMarket`) |
| **TODO** | Usage published live | stores opened, members, reserve, burned MSK and owner fees per model, read from the chain and shown on the explorer or the app, never typed into a README |
| **PARTIAL** | DNS finality | in Bootstrap on testnet-12 until validators are funded (launch note §2.3) |
| **TODO** | Faucet | testnet-12 faucet is unfunded ([join guide §4](testnet12-join-mining.md)) |
| **TODO** | Long soak with no consensus change | the plan exists ([testing/public-testnet-soak.md](testing/public-testnet-soak.md)); no public network has yet run 30 days on one ruleset |

## 5. Release

| status | item | evidence / what is missing |
|---|---|---|
| **PASS** | Pinned toolchain | [`rust-toolchain.toml`](../rust-toolchain.toml), enforced in every job by `scripts/ci-toolchain-pin-check.py` |
| **PASS** | Checksums published | per-binary sha256 in [the launch note](t12-launch-2026-09-25.md); `SHA256SUMS` on earlier GitHub releases |
| **PASS** | Component digests | [components manifest](components-manifest.md) (`misaka/components/v1`) is written on release |
| **PARTIAL** | Reproducible binaries | testnet-12's build matched byte-for-byte across two checkouts; nobody else has reproduced it, and CI does not check it |
| **PARTIAL** | Multi-platform binaries | CI builds and smoke-runs Windows. `deploy.yaml` builds x86_64 Linux, ARM64 Linux, x86_64 Windows and ARM64 macOS when a release is published. The ARM64 Linux leg has not run yet, and testnet-12 ships x86_64 Linux only |
| **TODO** | Tagged releases | testnet-12 shipped as commit `0e8ec984e` with no tag and no GitHub release; the last release is testnet-11's `testnet-main-e65ccf20` (2026-09-07) |
| **PARTIAL** | Signed artifacts | `deploy.yaml`'s `sign` job writes `SHA256SUMS`, signs it with Sigstore (`SHA256SUMS.sigstore.json`) and attests provenance for every file, and its `verify` job checks all of it from a clean runner ([release-process.md](release-process.md)). It has not had its first dry run yet, and no release has been cut with it |
| **TODO** | Signed tags | no release tag is signed yet; [release-process.md](release-process.md) §2 makes it a step |
| **PARTIAL** | SBOM | `deploy.yaml` writes an SPDX SBOM of the source tree on release; no release carries one yet |

---

## Path to v1.0.0 (proposed, not adopted)

This is a proposal for the maintainers. Nothing below is in force until they adopt it and say so
here.

1. **Stop new consensus features.** Land only testnet-12's scheduled fixes (launch note §2).
2. **Declare the freeze.** After the freeze, a consensus rule changes only to fix a Critical
   safety issue, and every such change is recorded in this file with its fence height.
3. **Cut `v0.9.0-rc.1`** as a signed tag ([release-process.md](release-process.md)). Attach Linux x86_64 and ARM64, Windows x86_64 and
   macOS ARM64 binaries, `SHA256SUMS`, its signature, an SBOM and the source archive. Operators run
   the tag, not `main`.
4. **Audit the tag.** Commission at least one independent, named review of the RC commit.
5. **Prove recovery.** Add fuzzing and the recovery drills in §3, run them in CI or on a schedule,
   and publish the results.
6. **Soak.** Run 30 days on one ruleset with no consensus change. `rc.2` only if a Critical fix
   forces one, and then the soak restarts.
7. **Final freeze** of genesis and parameters, then `v1.0.0` and the mainnet genesis.
