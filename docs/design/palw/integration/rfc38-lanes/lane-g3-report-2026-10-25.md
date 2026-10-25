# Lane G3 report (2026-10-25)

Base 44124edfe, branch claude/rfc38-lane-g3. Nothing pushed, no activation changed, no public node touched.

## 1. 15-gate runner (RFC-0006/0007/0008/0012 campaign gates)

Before: `scripts/drills/misaka-rfc-integration-campaign.py` + `rfc_integration_campaign.py` already freeze a plan, run one owned A/B/JOIN campaign once, and aggregate the 15 gates (`GATES`) with UNKNOWN for unobserved facts. No new runtime was created; three gaps were closed on that runner.

- Missing-input refusal (`rfc_gate_inputs.py`, new leaf, added to `SOURCE_LEAVES`): `k3-formal-inputs` (EXTERNAL) and `native39-rfc12-settlement-mapping` (POLICY) are registered as unavailable. `cap_gates` lowers a PASS to UNKNOWN with `missingExternalInputs` for the gates that need them (rfc06-model-boundaries-cost -> K3; rfc07-income-cost, rfc08-all-rewards-once, rfc12-market -> native39). FAIL is never hidden. The gate-to-input assignment is conservative and mine, not an adopted policy; revise when the inputs are specified.
- `--inputs manifest.json`: optional manifest; an entry naming an unavailable input is refused (no file can stand in for a missing specification).
- Failure preservation: each scenario row is appended and fsynced to `<root>/scenario-ledger.jsonl` in `CampaignDrill.scenario`'s finally, so a killed campaign keeps its failures.
- Resume: `--aggregate-root <root>` rebuilds scenarios/gates from the ledger without launching anything; torn last line ignored; unreached scenarios UNKNOWN; base-drill mechanics stay UNKNOWN; status is never PASS. A relaunch of the one-shot campaign remains refused by design.
- Tests: `python3 -m unittest tests.test_rfc_gate_inputs tests.test_rfc_integration_campaign` (cwd scripts/drills) -> 20 passed (4 new, 16 existing, including the import-closure pin).
- NOT done: no real multi-process run was performed (no frozen plan/candidate binary available in this lane); 100k, pruned IBD, OS crash not run. All 15 gates remain UNKNOWN; none updated.

## 2. RFC-0012 Latest/Safe/Finalized consumers

Survey: eth_getBlockByNumber/tag, eth_getBalance/Code/Storage/Nonce (`account_at`), eth_call/estimateGas (`snapshot_and_env_at`), and eth_getLogs from/toBlock (`resolve_block_number`) all resolve safe/finalized through `get_evm_canonical_heads` with explicit absence; latest/pending use the latest head; hash-pinned market/bridge-fill consumers and exact-head owner are recorded in the 0012-* evidence notes. Gas cap/empty/full parity code untouched.
- Closed: `eth_call`/`eth_estimateGas` at an absent safe/finalized head returned an invalid-params "unknown block" (caller error); now a server error "requested executed EVM head unavailable", matching `account_at`. Numeric/hash misses unchanged. File: `kaspad/src/eth_rpc.rs`.
- Test result: see the "Verification" section below.
- Not closed: positive nonBASE approved-work -> mature evidence -> native Safe/Finalized and the bridge/market fill on it need the native39 -> RFC-0012 mapping (POLICY, "not formulated"). Historical-signature replay needs real public-header replay against a fixed binary and is not claimed.

## 3. RFC-0009 post-checkpoint L2 / provider discovery

Not implemented, deliberately. The RFC lists provider discovery as DESIGN_GAP (line 285), and the general post-checkpoint L2 self-verification method (independent verification / trusted checkpoint / "sound proof system") is not specified beyond the trusted-checkpoint path that already exists (`t12_fork_choice_commitment`, CLI `--pin`, UNVERIFIED_REMOTE refusal). Choosing a discovery mechanism or a verification method would be inventing design (and a proof-system option is excluded by PRINCIPLES section 8). Needs an RFC/ADR decision first.

## Verification

- `CARGO_INCREMENTAL=0 buildslot.sh cargo test --offline -p kaspad state_selector` on 8504823dd + the eth_rpc.rs edit: exit 0; kaspad lib 4 passed, 0 failed, 0 ignored (existing selector tests; the build shows the edit compiles). The new absent-safe/finalized eth_call branch has no dedicated test (needs a consensus-session fixture): TEST remaining. Log: /private/tmp/claude-501/g3-kaspad-selector.log.
- Python: 20 passed (scripts/drills unittest). No full suite, clippy or fmt run. target/ removed.

## Remaining

- CODE: none found that is connectable without a new decision.
- TEST: real-process refusal matrix, 15-gate real campaign, historical-signature replay (fixed binary + scratch datadir).
- EXTERNAL: K3 formal inputs.
- POLICY: native39 mapping; RFC09 provider discovery and post-checkpoint L2 method.
