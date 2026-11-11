# BCT2 transport and local intake — 2026-11-11

Source `61adac48eef737f8832de6b74eec090360ab8a72`. [Summary and hashes](summary.json) preserve the raw checks, tests, source snapshot and binary identities.

Core tests (4) and mining test (1) passed. They check physical malformed/duplicate carrier counting, fixed receipt rent across UTXO handoff/refusal/retry, local refill/backpressure and the preallocation Selection geometry against the existing complete dependency graph. Core used a 64 MiB worker stack. Node, mining and P2P libraries passed `cargo check` after the final memory formula correction. The initial generated NativeHeap omission and both checks remain in the raw logs.

Commands use `CARGO_TARGET_DIR=/tmp/misaka-public-contract-test.fd7edK CARGO_INCREMENTAL=0` and `--config 'profile.dev.package.kaspa-consensus-core.opt-level=0'`. Tests: `RUST_MIN_STACK=67108864 cargo ... test --locked -p kaspa-consensus-core -p kaspa-mining --lib bct2_ -j2 -- --nocapture`. Check: `cargo ... check --locked -p kaspa-consensus -p kaspa-mining -p kaspa-p2p-flows --lib -j2`.

Transport now connects its own source-DAA fence, physical source/candidate counter, fee projection, typed UTXO handoff, owned rehearsal and authoritative dispatcher. Verify uses the existing fixed retained-control rent; Terminal retains no new receipt. Invalid/repeated syntactically valid Verify may still pay this rent, matching existing BCT1 Stop fee semantics, not the separate TKR1 success-only Open charge. All shipped policies remain `None`.

The memory formula accounts for retained handoffs plus one replay workspace and original/current/checkpoint state owners. It does not establish that a funded claim's later terminal will remain within the prefix cap after unrelated state growth. Existing terminal full-state scans/checkpoints require claim-local indexing/undo or a separately established admission guarantee before public qualification.

Selection geometry is source-only and preserves FullIds + every tile of the selected row times the existing supplier-role count. It creates no notice, supplier authority, reservation or adjudication. Full Selection/DA planner/runtime and a valid paid BCT2 receipt/terminal execution are still pending. This record is not public acceptance, Final or performance qualification.
