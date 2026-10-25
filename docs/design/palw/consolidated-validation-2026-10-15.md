# Consolidated validation, 2026-10-15

**Status: paused because RFC-0015 code was added to the scope.** The results below apply
only to the recorded commits. The whole phase reruns from step 0 once all RFC code, including
RFC-0015, is integrated.

Branch `integ/rfc02-rfc06-12-g14`, own worktree and `./target`, toolchain 1.93.0, `-j 8`, one cargo
invocation at a time. Environment for the whole phase: `CARGO_PROFILE_DEV_DEBUG=line-tables-only`,
`CARGO_PROFILE_TEST_DEBUG=line-tables-only` and `CARGO_INCREMENTAL=0`. Logs are under
`target/consolidated-validation/` (local, not committed). Each rung is checked against a 30 GB
free-disk floor; free space stayed between 135 and 141 GB throughout.

Start commit: `fa7b1311f` (G14 prefix history on p2p 108, `PROTOCOL_VERSION = 108`).
Fix commits made during the run:
- `1153e203b`: native resource scanner and generator fixes.
- `350acd0b4`: compile fixes across the crates.

## Step 0: generated sources are reproducible

| Command | Source | Result |
| --- | --- | --- |
| `python3 scripts/drills/gen_rfc06_native_wire.py`, then `rustfmt` (with the repo `rustfmt.toml`, edition 2024) on `generated.rs`, `wire_generated.rs`, `wire_kernel_aux.rs`, `kernel_ledger.rs`, `wire_budget.rs`, `ledger_rows_generated.rs`, `private/palw_state_v2.rs` and `private/wire_carriage.rs`, then `git diff --exit-code` | `fa7b1311f` | PASS |
| same, after the generator fixes | `1153e203b` | PASS (byte-identical regeneration) |

Do not pass `--edition 2021` to rustfmt: it ignores `rustfmt.toml` and reorders imports.
Never run rustfmt on a crate root (`lib.rs`): it recurses into every module.

## Step 1: `cargo check` ladder, bottom-up

All rungs use `--locked -j 8`. "Fixes" names what had to change before the rung went green;
the details are in the two fix commits.

| # | Command | Result | Fixes |
| --- | --- | --- | --- |
| 1 | `-p misaka-palw-tir --all-targets` | PASS | none |
| 2 | `-p misaka-palw-kernel --all-targets` | PASS | out-of-scope `id` in the ledger default path; `VerifiedWorkBeaconV1` re-export |
| 3 | `-p misaka-palw-challenge --all-targets` | PASS | none |
| 4 | `-p kaspa-consensus-core --features evm,private-integration-config,legacy-secp256k1 --all-targets` | PASS | about 40 distinct errors (see below) |
| 5 | `-p kaspa-consensus --features evm,private-integration-config,legacy-secp256k1 --all-targets` | PASS | consensus/remote/base0/RPC/SDK/test fixes (see below) |
| 6 | `-p kaspa-mining --features evm --all-targets` | PASS | none |
| 7 | `-p kaspa-rpc-core -p kaspa-grpc-core -p kaspa-grpc-server -p kaspa-grpc-client -p kaspa-wrpc-server -p kaspa-wrpc-client -p kaspa-rpc-service --all-targets` | PASS | none |
| 8 | `-p kaspa-p2p-flows --features evm --all-targets` | PASS | none |
| 9 | `-p kaspa-consensusmanager --all-targets` | PASS | none |
| 10 | `-p misaka-palw-tir-exec --features node --all-targets` | PASS | none |
| 11 | `-p misaka-palw-remote --features rpc --all-targets` | PASS | `RpcApi` import path |
| 12 | `-p misaka-palw-sdk --bins --tests` | PASS | none |
| 13 | `-p misaka-palw-base0 -p misaka-palw-gateway -p kaspa-pq-validator-core --all-targets` | PASS | unqualified `step_tile_leaf_hash_ctx_v1` in a base0 test |
| 14 | `-p kaspa-wallet-core --all-targets` | PASS | `RpcApi` import path |
| 15 | `-p kaspa-wallet-core --target wasm32-unknown-unknown --features wasm32-sdk`, with `CC_wasm32_unknown_unknown` and `AR_wasm32_unknown_unknown` pointing at Homebrew LLVM | PASS | environment only: Apple clang has no wasm target |
| 16 | `-p misaka-cli --all-targets` | PASS | none |
| 17 | `-p kaspad --features evm,private-integration-config --lib --bins --tests` | PASS | `.arg(a, b, …)` changed to `.args([..])`; `dtype_of_tag` made pub; test ledger clone |
| 18 | `-p kaspa-testing-integration --features evm --tests` | PASS | none |

**Rerun note.** Rungs 1–4 were last run before two later additive edits: the kernel
`dtype_of_tag` became `pub`, and the challenge crate re-exports `ImplementedV1`. Rung 17
compiles both libraries, but not their test targets, against the final tree. The resumed run
repeats the whole ladder on the final commit.

Classes of compile defect fixed in rungs 4–5:
- Generator output errors:
  - `scan_native_aux_row_v1` was missing its `Ok(..)` wrap;
  - `size_of::<..>>>()` had an extra `>`;
  - the Panel container field types were not rooted;
  - there was no `Arc<T>` wire scanner;
  - the flat candidate modules are now re-exported at the kernel crate root.
- `route.ledger()` now returns `ReservedKernelLedgerV1`. Call sites deref it, iterate it by
  reference, or return the reserved ledger; tests take an explicit `KernelLedgerV1::clone`.
- Private-field reads now go through accessors: `claim`, `tir_class_v1`, and the new
  `staged_court_row_v1`.
- Visibility: `bond_budget_op` is `pub(super)`, `lanes_read` is `pub(crate)`, and
  `palw_state_v2_tip` is `pub(crate)`.
- API drift:
  - `CompressedParents::get(0)` replaces `first()`;
  - `IterateBounds::into_bounds`;
  - `services.reachability_service`;
  - economic evidence validates through `net_receipt()`;
  - RPC `Vec<Serializer>` uses `serialize!`/`deserialize!`.
- A non-exhaustive `apply_to` match: the Float32, native-selected and TIR-prefix route forms
  joined the bundle-control arm.
- Lifetime fixes:
  - base0 sidecar check through a borrowed `check_v1`;
  - a snapshot admission error built from an owned `String`.
- A duplicated `pub(crate) mod tests {` line from the G14 merge was removed.

Open for the clippy step: the libraries still emit warnings, for example unused imports in the
native resource modules, an unreachable pattern in kernel `stage14/bounded28.rs`, and an unused
`check_stage_v14`. They were not fixed yet.

## Steps 2–4: not started

Focused tests, affected suites, pytest, clippy and the pre-existing-failure classification
against base `64699e960` were not started before the pause.
