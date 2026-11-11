# Plain Output quote and submission — 2026-11-11

Source `abfe8ef6ad95786e5d0f23e1a79c1ab1f94a4b11` connects CLI `batch-verify-plain-output` and `batch-convict-plain-output`. Both default to preview and require `--submit` to broadcast at the exact booked DAA. Verify purpose-signs the accepted FullIds request; Terminal refers to a prior fault receipt. Both fund the native BCT2 transaction from a mature UTXO, use the core rent projection, enforce the supplied maximum fee, and re-read the unchanged quote after signing and funding. Output keeps `acceptanceConfirmed=false`, including after mempool submission.

The Terminal selector `convict-plain-output-v2` uses the same authenticated selected-parent reconstruction, virtual/import recheck and accepted-root identity as Verify. A prior fault receipt, unexpired booking and active transport policy at that slot are required. It does not make Holds terminal-eligible.

Node/RPC/CLI `cargo check` passed. Two CLI tests verify substituted claim/notice/network/parent/signing digest, expired booking and exact submission slot refusal. One RPC selector test verifies bounded exclusive interpretation, including rejected session/target/ordinal combinations. [Summary, hashes and limits](summary.json) identify source, binaries and raw logs.

Commands used `CARGO_TARGET_DIR=/tmp/misaka-public-contract-test.fd7edK CARGO_INCREMENTAL=0`, `--config 'profile.dev.package.kaspa-consensus-core.opt-level=0'`, `--locked` and `-j2`. Check: `check -p kaspa-consensus -p kaspa-rpc-service -p misaka-cli`. CLI: `test -p misaka-cli --bin misaka plain_output_ -- --nocapture` with `RUST_MIN_STACK=67108864`. RPC library test binary ran `numerical_batch_open_selector_is_bounded_and_exclusive --nocapture` after building `test -p kaspa-rpc-service -p kaspa-consensus-core --lib --no-run`.

These are signing/transport integration checks, not public receipt/Final evidence. No CLI transaction was sent by these tests. The dedicated plain FullIds Open quote, complete Selection/DA planner/runtime and bounded terminal settlement remain outstanding. Public policies are unchanged and inactive.
