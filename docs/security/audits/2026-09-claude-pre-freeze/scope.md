# Scope — 2026-09 Claude-assisted pre-freeze security review

**Review type: AI-assisted security review.** This is not an independent third-party audit and not
a proof of security. The review was run with Claude (Anthropic) under the maintainers' direction.
See [methodology.md](methodology.md) for how findings were produced, verified and graded.

## 1. Audit target (fixed before any review started)

| field | value |
|---|---|
| Repository | `MISAKA-BTC/misakas` |
| Commit | `3d2bd6dc5d77d37396d1b73c4c13526090923b92` (`main`, merge of PR #116, 2026-09-26 00:52 +0900) |
| Branch audited | `main` at the commit above. Moving `main` is **not** audited |
| Network | `testnet-12` (R-core+) |
| Consensus params fingerprint | `b8564b888e55bb5f797e708a3f65e7cd122065123a3ab09cbeb8d10c98715d8f` |
| Consensus schedule id | `93da24cc60f7a77e63c43106e96298d2979644a3fc0c3f82529c8849333127fd` |
| Genesis | `a27f8f44fe4d91a5bed940be9dbd6d260ccb95cc00d948b1c08ddb6bd1a5f02542a6cf35c7a4d959ba4863ac1557861671763e5cc22937c697870283a8ca1f23` |
| `consensus_frozen` (`release.json`) | `false` — this is a **pre-freeze** review |
| Rust toolchain | `rustc 1.93.0 (254b59607 2026-01-19)`, `cargo 1.93.0 (083ac5135 2025-12-15)` (pinned by `rust-toolchain.toml`) |
| `Cargo.lock` sha256 | `1a92b5f49f9616d1abfb928c5cb2a06a6b06c23cec72728c483bee855b7860c3` |
| `release.json` sha256 | `9ab75af237bbf21189e27867e17df31dd8b62c531268e2330950f0dd36d9f0ef` |
| Review started | 2026-09-25 (UTC) |

### 1.1 Relation to the testnet-12 release commit

testnet-12 shipped as commit `0e8ec984efc97cecb000b531fc53647b1bf13e08`
([launch note](../../../t12-launch-2026-09-25.md)). The audited commit is 13 commits after it.
Between the two, **no Rust source, `Cargo.toml` or `Cargo.lock` changed**:

```
$ git diff --stat 0e8ec984e 3d2bd6dc -- '*.rs' 'Cargo.toml' '**/Cargo.toml' 'Cargo.lock'
(empty)
```

The 37 files that did change are documentation, the explorer front end, `.config/nextest.toml`,
`release.json` and the release tooling (`.github/workflows/ci.yaml`, `.github/workflows/deploy.yaml`,
`scripts/misaka-release-*.py`, `scripts/misaka-ci-summary.py`). The consensus code reviewed here is
therefore byte-identical to what testnet-12 runs; the release tooling reviewed is the newer one.

### 1.2 How the identity was checked

The fingerprint, schedule id and genesis above are those declared in `release.json` and the launch
note. They are pinned in-tree by
`consensus/core/tests/palw_clock_lead_cap_is_t12_only.rs` (`T12_WITH_THE_CAP`), which the review
ran against the audited commit (result recorded in [README.md](README.md) §Commands). The review did
not start a `kaspad` binary to read the identity it prints.

## 2. Parameter sets that decide reachability

A finding's severity is graded against the rules a node **actually runs**, read at runtime from the
shipped parameter constructors in `consensus/core/src/config/params.rs` — never from test fixtures
or preset constants that a constructor later overrides:

| network | constructor | note |
|---|---|---|
| testnet-12 | `palw_t12_shipped_params()` | arms every rule at DAA 0 via `palw_t12_arm_every_rule_from_genesis`, except bond maturity (DAA 1,000) and six dormant rules (`palw_inactivity_leak`, `palw_frontier_provenance`, `palw_beacon_fold`, `palw_shard_licensing`, `palw_fp_decode_rules`, `palw_fp_decode_constraint`) |
| mainnet (preset, not final) | `mainnet_shipped_params()` | defined but not declared final; findings here are graded as pre-mainnet |
| testnet-11 | `palw_rc_shipped_params()` | reference only; `main` cannot join testnet-11 |

## 3. In scope

The review was split into domains, each run by agents with no access to the other domains' work
(see [methodology.md](methodology.md) §3). The units each domain actually covered, and what each
unit left out, are recorded in its findings file.

| domain | findings file | code |
|---|---|---|
| Architecture and threat model | [threat-model-review.md](threat-model-review.md) | ADRs, `docs/palw-rc-threat-model.md`, `docs/architecture/overview.md`, checked against code |
| Consensus | [consensus-findings.md](consensus-findings.md) | `consensus/`, `consensus/core/` (non-PALW-economic parts), fork choice, clock, DAA, fences, fingerprint, genesis, pruning / IBD, DNS finality, EVM lane consensus |
| PALW and economics | [palw-findings.md](palw-findings.md) | `consensus/core/src/palw_*` (attempt, admission, panel, court, settlement, reward, bond, registry, model market, free prompt, execution lane), `misaka-palw-base0`, `misaka-palw-gateway`, `misaka-palw-constraint`, `misaka-palw-fp-submit` |
| Cryptography and PQ | [crypto-findings.md](crypto-findings.md) | `crypto/`, `consensus/core/src/mldsa87_primitives.rs`, `consensus/core/src/hashing/`, signing domains and contexts, randomness and seed derivation |
| P2P, RPC and operator | [network-findings.md](network-findings.md) | `protocol/`, `components/`, `mining/`, `rpc/`, `kaspad/src/palw_*` node duties, `kaspa-pq-validator*`, `kaspa-pq-signer`, `misaka-cli` key handling |
| Release and supply chain | [release-findings.md](release-findings.md) | `.github/workflows/`, `scripts/ci-gates.sh`, release scripts, `release.json`, `deny.toml`, `Cargo.lock`, `build.rs` scripts, `docker/`, `contrib/t12-deploy-kit/` |
| Prior audit closure | [prior-audit-closure.md](prior-audit-closure.md) | every Critical / High of the in-tree audits, re-checked against code and regression tests |

## 4. Out of scope

- Any commit other than the one in §1, including later `main`.
- Live network state, deployed hosts, seeders and explorers (`misakascan.com`, `misakaoptions.com`).
  The review read code only; it made no request to any MISAKA host.
- The web front ends (`web/`, `contrib/misakascan-t12/`) except where they build transactions.
- Upstream Kaspa code that MISAKA did not change, except where a MISAKA change alters how it is
  reached.
- Legacy crates marked legacy in `docs/architecture/overview.md` §3 (`misaka-palw-worker`,
  `misaka-palw-agent`, `misaka-palw-pow-driver`, `misaka-palw-shadow`, `pq-miner`, `misaminer`,
  `bridge`), except for secrets handling and anything reachable from `kaspad`.
- Formal verification, economic simulation at network scale, and cryptographic proofs of the
  underlying primitives (ML-DSA-87, BLAKE2b, SHA3). These are listed as areas needing human review.

## 5. Publicly known issues at the audited commit

These were disclosed in the [launch note §2](../../../t12-launch-2026-09-25.md) before the review
began. The review does not re-report them as new. A finding with the same root cause carries a
`Known issue` reference; a finding that shows a known issue is wider than disclosed is reported.

1. Heartbeat-transparency double spend (CRITICAL, branch `rcore/hb-transparency-fix`).
2. Panel-seed grinding by the anchor producer (CRITICAL, branch `rcore/panel-seed-exec-commitment`).
3. DNS finality in Bootstrap until validators are funded (the stake reorg gate is not in force).
4. Post-`Final` `Valid` locks predicted to fill a seat's 500 ‰ budget.
5. V04: licences held back by claim-id order.
6. V01 / V03 / V05 / V07 readiness and carrier recovery rules.
7. Scheduled: registry resilience, bridge audit BR-1…BR-7, position fixes.
