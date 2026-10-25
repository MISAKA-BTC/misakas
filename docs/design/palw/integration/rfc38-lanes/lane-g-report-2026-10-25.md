# Lane G report (2026-10-25, successor session)

Base fdd468a5b, branch claude/rfc38-lane-g2.

## Closed by this diff
- RFC-0003/0004/0022 deferred-feature refusal, fold level only: `consensus/core/tests/palw_deferred_feature_fold_matrix.rs` (taken from the predecessor WIP, reviewed, unchanged).
  - 14 improvement objects (tags 70-82 + case-key reveal 86), 19 unarmed rulesets, DAA 0..u64::MAX/2, singly and mixed: each refused by name ("before palw_improvement_v1 is in force"), input state unchanged, replay refuses identically.
  - t12 rows (`palw_t12_shipped_params`, `from(testnet-12)`): fence armed exactly at `PALW_T12_INT11_FLAG_DAY_DAA`, refused below it (checked against the real Params via `improve_active_at`).
- Test: `CARGO_INCREMENTAL=0 buildslot.sh cargo test --offline -p kaspa-consensus-core --test palw_deferred_feature_fold_matrix` -> exit 0, 2 passed, 0 failed, 0 ignored. Log: /private/tmp/claude-501/g2-matrix.log.

## Not claimed
- This is an in-process fold check. It is NOT a real-process RPC / restart / resync / IBD refusal run; those remain TEST (need fixed binary + scratch datadir).
- Rows armed at/after the int-11 flag day are only asserted below H; behaviour at/after H is not a refusal and is not covered here.

## Remaining
- TEST: real-process refusal matrix; 15-gate runner manifest/failure-save/resume/UNKNOWN aggregation (not started; all gates stay UNKNOWN); RFC12 consumer wiring survey (not started).
- CODE: RFC09 post-checkpoint L2 verification/provider discovery (not started).
- EXTERNAL/POLICY: K3 inputs, native39 mapping unchanged.
- No public node touched; nothing pushed; no activation changed.
