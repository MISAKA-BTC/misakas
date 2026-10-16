# Consolidated validation, 2026-10-15/16

Branch `integ/validation` (own worktree and `./target`), toolchain 1.93.0, `-j 8`, one cargo invocation at a time,
`CARGO_PROFILE_DEV_DEBUG=line-tables-only CARGO_PROFILE_TEST_DEBUG=line-tables-only CARGO_INCREMENTAL=0`. Logs are under
`target/consolidated-validation/` (local, not committed). Integration heads folded in: `7d0eaa6b2`, `f244ed92d`, `12fb03fd4`,
`e72c1870d`, `5ccc3a234` (last merge `1fc876ebd`; head `dcd255e93`). Only green runs are labelled PASS.

## Step 0: generated sources are reproducible

`python3 scripts/drills/gen_rfc06_native_wire.py`, rustfmt (repo config, edition 2024) on the eight generated files,
`git diff --exit-code`.

| Source | Result |
| --- | --- |
| `3787e600e`, `08ac332ab`, `3730427fe`, `8d568dca4` | PASS |
| `31186b2f8` (e72c1870d merge) | FAIL: aux-143 rows hand-added to generated files; fixed in the generator inputs (`417f3dcff`) |
| `417f3dcff`, `1fc876ebd` | PASS |

## Step 1: check ladder (20 rungs, `--locked`)

Rungs: tir, kernel, challenge, consensus-core, consensus, mining, rpc×7, p2p-flows, consensusmanager, tir-exec `node`,
remote `rpc`, sdk, tir-artifact, tir-lower `remote`, base0/gateway/pq-validator, wallet-core, wallet-core wasm32,
misaka-cli, kaspad (lib/bins/tests), testing-integration.

| Run | Source | Result |
| --- | --- | --- |
| L7 (`L7-r*.log`) | `3730427fe` | PASS 20/20 |
| L9 (`L9-r*.log`) | `417f3dcff` (rungs 1–7), `f2b8b05b8` (8–20) | PASS 20/20 |

## Step 2/3: tests (nextest, `--no-fail-fast`)

| Run (log) | Source | Run | Pass | Fail | Notes |
| --- | --- | --- | --- | --- | --- |
| focused tir/kernel/challenge/panel/tir-artifact/tir-lower/base0 (`v7-focused`) | `3730427fe` | 2481 | 2453 | 28 | fixed 18 (`b8d8ab1a3`), 7 pre-existing on base `64699e960`, 3 lane-open (below) |
| consensus-core full (`v7-core`) | `b8d8ab1a3` | 6059 | 6038 | 21 | fixes `664efaa6d`, `e95166f12`, `b57628bdd`; G14 conviction tests fixed by the lane at 12fb03fd4 |
| kaspa-consensus full (`v7-consensus`) | `664efaa6d` | 1050 | 987 | 63 | fixes `2f63cd963`, `e95166f12`; open below |
| kaspad, mining, p2p-flows, sdk, remote (`v7-affected`) | `e95166f12` | 1685 | 1658 | 27 | 4 = debug-runtime tests stopped (SIGTERM); fixes `b57628bdd`, `8275f2188` |
| testing-integration rpc_tests (`v9-misc`) | `f2b8b05b8` | 1 | 1 | 0 | PASS |
| G14 lanes (`v9-g14`) | `417f3dcff` | 141 | 138 | 3 | 3 = conformance beacon (below) |
| requested set (`v10-req`) | `1fc876ebd` | 150 | 150 | 0 | PASS: free_prompt 5, g14_close_legacy 8, g14_close_gates 12, lg14a_legacy_dispute_fold 9, tir_fp_lane 6, palw_tir_public 69, palw_legacy_public_filer_v1 12, palw_memory_routing_v2 9, lane_closure 5, lg14a_legacy_filer_e2e 14 |
| `python3 -m pytest scripts/drills/tests` | `3730427fe` | 129 | 129 | 0 | PASS |
| python `--self-test`s (10 scripts) | `3730427fe` | 10 | 9 | 1 | `t12_repin.py` needs harvested logs (environment) |
| node `--test` web/contrib | `3730427fe` | 11 | 11 | 0 | PASS |

## t12 identity (all new fences dormant)

`palw_t12_flag_day_9000` (int-12 params id `5ee7fd8e…`, identity, schedule, fork id), `the_int12_kind_list_is_frozen`,
`every_int12_wire_type_is_unchanged_or_classified`: PASS (`pins1`, `08ac332ab`); `g14_close_gates::testnet12_identity_did_not_move`:
PASS at `8d568dca4` and `1fc876ebd`. The int-12 variant digest `0x01d641040ea4a142` is unchanged. No drift.
A-2 new-kind pins re-taken from the Rust test (`664efaa6d`^): 240 `0x91362a7791c31616`, 241 `0x5a27c8d5601adf8e`,
242 `0x3afba613f2ab70a6`; 170 `0xb68d62123647ff28` and 171 `0x68623b7dc7d30b55` equal the Python port.

## Still failing (not fixed here)

- **Kernel-route beacon cannot lock (≈45 consensus tests, `g14_kernel_route_e2e::{conformance,opv_bootstrap}`, core
  `rfc0010_production_fold::paid_panel_sources…`).** Since `0a11986ed` no kernel Final exports a beacon event
  (`palw_kernel_route_v1.rs` ~808: every path `None` "unless an authenticated selected-approval adapter supplies an event").
  The fixtures need that adapter (RFC07 remaining work); no fixture change can lock the beacon.
- RFC-0018 `rfc0018_dag_siblings` ×3 (unbuilt lane source: the armed fence is refused by the binary; card 2's claim unrecorded).
- `rfc9_v4_chain_e2e` offline miner: hash-dependent count of winning quanta (1, test needs 2).
- `rfc0016_e2e::t12_rfc0016_zero_rights_heartbeat_keeps_the_cadence` (floor-class harness, exempt by design).
- Pre-existing on base `64699e960`: rfc12 ×4, `t12_a2u_mixed_verdicts` ×2, SDK ×3 (qwen25 pilot bytes, recorded 32-position,
  corpus preflight), tir-lower ×4 (three_way, corpus_v2, admission, quant_decode_pins), base0 float_free ×2 and prefix cache.
- kaspad service E2E cluster (16: pruned-import mismatch, view None, honest job flagged, socket/wall deadline; SIGABRTs are a
  DB-lifetime panic during cleanup after those).
- Lane-open: base0 `gdn_evidence_v2_hybrid` forged checkpoint (now reaches `PromptIdsAreNotTheJobs`), kernel
  `native_invocation32_stream` (program violates TIR NF-15/NF-19).
- Debug-runtime, stopped: SDK `k2s_v5_the_worst_geometry…`, `adv_c4_conformance` byte flips, `gen_sd3_class`, one preflight.

## Clippy: NOT green

`cargo clippy --workspace --all-targets --features evm,private-integration-config,legacy-secp256k1 --keep-going`
(`v9-clippy-list`, `8275f2188`): `misaka-palw-worker` build script needs `MISAKA_LLAMA_SRC` (not a default member;
must be excluded); `simpa` did not compile (fixed `dcd255e93`); 557 warnings (222 consensus-core, 74 kernel, 56 SDK,
60 kaspad TIR acquisition/scheduler, …): collapsible ifs 67, needless borrows 35, never-used items ≈80, unused imports 37,
unnecessary parentheses 33, duplicated attributes 24, unused mut 23, items after test module 17, … Not fixed.

## Fix commits (this phase)

`305373db9`, `78cb8cfb2`, `b03945c66`, `08ac332ab`, A-2 re-pin, `b8d8ab1a3`, `664efaa6d`, `2f63cd963`, `e95166f12`, `b57628bdd`,
`417f3dcff`, `f2b8b05b8`, `8275f2188`, `dcd255e93`.
