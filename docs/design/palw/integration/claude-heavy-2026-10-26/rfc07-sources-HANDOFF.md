# RFC07 normal V5 source chain — HANDOFF (Claude, 2026-10-26)

Branch `claude/heavy-rfc07-sources`, base `2de0a526b41559994e9894e6e13a268bb4dc2247`.
Not pushed. No activation, economics, wire ID or DB schema change.

## Commits
- `719f3cfa9` Codex WIP 7 files copied verbatim (SHA256 = rfc07-input-manifest.json, all matched).
- `cac06a35a` staged patch `normal-source-native-tests.patch` (all 4 base SHAs matched; applied directly, staged SHAs match stage-review.json) + fixture fixes.
- `291e015df` native caller wiring.

## Requirements closed
- Native callers: ordinary Receipt (`receipt_fold`) and FS initial-source revalidation (`round_message`) in
  `palw_claim_challenge_fold_v2.rs` call `paid_claim_check_authority_checked_v2` and map through
  `kernel_ledger_projection_state_error_v1` (Resource -> `NativeResource`, Decode -> refusal).
- `paid_claim_check_authority_checked_v2` normal path uses `rfc07_normal_paid_material_owned_v5` (same V5 collector,
  same provenance/approval). The material and its source/promise tickets live until the collector's last use (explicit `drop`).
- `paid_claim_check_authority_v2` (String) is now only a display wrapper over the checked read (SDK edge,
  `misaka/palw/sdk/src/claim_paid_v2.rs:138`). The old body's `if let Ok(...)` collapse of a v6 read error is gone.
- Initial/staged: only true primary absence falls to staged; present corruption/Resource does not (test passes).
- Authority Resource fixture (`paid_check_authority_resource_v2.rs`): the 512 MiB logical limit was refused in the first
  reads (fixture error, not RED). Ceiling now `1 << 40` logical; the hook's independent owner takes the real remainder,
  so only the second read sees Resource. Both children now reach the hook and pass.
- Native Fund / FundRound fixture: kernel `fee_gate` needs `optimistic_allowed`; without OPV the inner refusal is a silent
  no-op and the outer "strict ... debit failed" fired. OPV must bind the route from genesis
  ("folded under another policy" otherwise), so `setup_mode_contract_opv` was added. OPV terms = existing
  `PalwPanelFreeFenceV1::at(always).opv_policy()` (interim fixture terms; not an approval or economics value).

## Not done (do not treat as done)
- Nonempty accepted source inventory: no existing accepted-source fixture found in `palw_state_v2/tests`. The 7 tests cover an
  empty source inventory only; source/promise ticket last-use with a nonempty set and source independence are NOT shown.
- Normal V5 material path is not exercised by a fold-level Resource test (the Receipt/FS Resource tests take the seeded/bootstrap branch).
- Collector internal workset, validator scratch, Panel raw boundary (`panel_source_fee_authority_v2` stays String/raw): not typed.
- `rfc07_normal_source_projection_draft.rs` is left unregistered (superseded by the owned draft; not a duplicate implementation).
- K3, native39 settlement mapping, 15 UNKNOWN gates: untouched.

## Verification (rustc 1.93.0, buildslot, CARGO_INCREMENTAL=0, debug=0, consensus-core opt-level=0, TMPDIR=/private/tmp)
Command:
`buildslot.sh env CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test --offline -p kaspa-consensus-core -p misaka-palw-sdk --lib --config profile.test.package.kaspa-consensus-core.opt-level=0 -- --test-threads=1 rfc07_normal_owned_ paid_check_authority_ paid_claim_reader_v1 claim_paid`
- exit 101. kaspa-consensus-core: **22 parent passed / 0 failed** (7 rfc07_normal_owned_, 4 paid_check_authority_, rest of
  paid_claim_reader_v1; isolated children each "running 1 test", ok). Binary
  `kaspa_consensus_core-ac47ea72040d420f` sha256 `787c15466126329d868e3f6bde1ea956699d319a7ed85c0baf02625c6e89113b`.
- misaka-palw-sdk: 24 passed / **1 failed**: `material_generation_v7::source_controls::continuation_keeps_original_index16...`
  (`left 17 right 16`, wire enum index assertion in an untouched file; pre-existing on this base, unrelated to this change —統括の確認要).
  Binary `misaka_palw_sdk-aa39a27296544d76` sha256 `bc2c40e6166d46f7293e7c0f356c92f8aa58b238c52d937534d46e371b78fdce`.
- Logs (scratchpad, not committed): final.log, run3.log. `target/` deleted afterwards.
- PENDING: kaspa-consensus / kaspad builds, clippy, full suite.
