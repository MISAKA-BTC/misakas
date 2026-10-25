# RFC12 public-history census and observed release provenance — 2026-10-25

## Actual observation

Read-only SSH to the existing `misaka-ibm` alias and localhost wRPC JSON port26314 acquired real public testnet-12 header/verbose pages. No remote file, setting, process restart, deployment or direct database access was performed. A single serial reader continues under its original bounded job (1200pages/2400seconds); no duplicate scan was launched. The collector emits canonicalized response SHA plus compact header/verbose records, not a complete block-body archive or a signed history artifact.

At **2026-10-25T10:26:16.185942+00:00**, **81 complete pages / 14673 unique blocks / maximum DAA2236** were present. The observed executable-history ledger reports **2745 round verdicts UNKNOWN**, no granted/refused verdicts in this partial scope. These are observed algo10 blocks; they are not evidence of actual accepted round permits. The fixed anchor is `af1fbb4db37e5e98106cd75e1f9a0a92d5efff0448734036e620b88286d8041a283cb5e01616630a89db45db7eebdea85a3195a2735f669050f219be223480e2` (DAA15477 in the initial inventory). The scan has not reached it. Anchor ancestor closure, the selected-parent chain and end-of-scan generation/canonical checks are not complete. **Coverage UNKNOWN_PARTIAL; cross-version EVM compatibility UNKNOWN.**

The latest valid root observation confirmed the same SSH process PID23593 alive, with advancing complete pages. Its owning agent `/root/rfc13_25` can poll exec session51660; the root executor cannot poll that agent-scoped handle directly. An inaccessible root handle is not a terminal process. Future continuations must check the original handle/process and current file before starting any replacement. Source/analyzer SHA, the before stamp and exact current counts are in [the interim record](0012-public-history-census-interim-2026-10-25.json). This dated snapshot will not automatically update with the live file.

## Why round telemetry cannot decide compatibility

`rpc/service/src/palw_lane_view.rs` classifies algo10 in mergeset rows; `palwMergeView.roundBlocks` is not an accepted-permit index. Verdicts come from the bounded, in-memory telemetry ledger (`palw_exec_view_v1` and `palw_record_round_verdicts`), which can return UNKNOWN after eviction/restart. Unknown is neither refused nor zero permits. Native getBlock exposes EVM commitment/payload hashes but the observed RPC does not expose the derived EVM header gas limit. An independent upgraded replay or retained derived-header evidence is still needed for old30M versus required33M history decisions.

## Observed release differs from the implementation branch

The running executable path is `/root/t12-rel/247ade1f90b6/bin/kaspad`. GitHub resolves that tag to source commit **247ade1f90b691e7a83ab890519fcf83118e2f2b**, whose message and diff schedule the separate int-13 list at DAA15500. This is not the local full-activation fence, which remains None. The path tag locates source; it does not by itself authenticate a reproducible build or binary/source identity.

The source at that exact commit has a full executor user-gas adjustment, while its empty fast path constructs the header from the base derived environment without that adjustment. This is relevant source evidence, not proof that a particular public block exercises the omission. The local branch's extracted `evm_market_owned_v2.rs` and `config/params/full_activation_v1.rs` do not exist at that remote ref, confirming that the release source must not be silently equated with this worktree. Exact Git blob identifiers and bounded source excerpts are in [the provenance record](0012-public-history-runtime-provenance-2026-10-25.json). Source: [the observed release commit](https://github.com/MISAKA-BTC/misakas/commit/247ade1f90b691e7a83ab890519fcf83118e2f2b).

No remote changes, public activation change, int-13 adoption, merge or binary replacement is made by this census. It does not complete historical signatures, full native39 settlement, real K3 work, source independence or any of the15 UNKNOWN campaign gates.

## Local raw artifacts

- `/tmp/misaka-public-history-census-readonly-20261025.py`: exact stdin reader script.
- `/tmp/misaka-public-history-census-20261025.ndjson`: live compact records; inspect completed lines only.
- `/tmp/misaka-public-history-census-analyze-20261025.py`: anchor closure/generation/unknown analysis.
- `/tmp/misaka-public-history-census-summary-root-20261025.json`: root snapshot underlying this report.
- `/tmp/misaka-ibm-history-rpc-inventory-20261025.json`: three-node initial RPC inventory.

The process is intentionally bounded. A budget/error terminal result remains partial unless all anchor closure and identity checks succeed. A later full enumeration still does not imply EVM replay compatibility PASS.
