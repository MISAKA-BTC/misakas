# Archive

Dated engineering records kept as evidence: audits, measurements, drills, superseded plans and completed migrations. Each file describes the tree and the network **as they were on the date it names**. Hashes, class IDs, DAA heights, peers and commands in these files are not current values, and nothing here is maintained.

For the current system use the [documentation map](../README.md), the [ADRs](../adr/README.md), `--help` and chain RPC.

| kind | files |
|---|---|
| mainnet audits (Aug–Sep 2026) | `palw-mainnet-*audit*-2026-*.md`, `palw-external-audit-2026-08-21.md`, `palw-critical-audit-2026-08-19-ja.md`, `palw-only-v4-audit-2026-08-17-ja.md`, `ambient-pol-binary-audit-2026-08-15.md`, `audit-scope-evm-rpc.md`, `audit/` (the 2026-09-19 LLM-mining reward probe, cited by `consensus/core/src/palw_reward_properties_v1.rs`) |
| measurements and benches | `palw-algo4-*`, `palw-aarch64-*`, `palw-base0-*`, `palw-legs-capture-*`, `palw-llm-pow-block-generation-*`, `palw-stage0-fleet-replay-bench-*`, `palw-second-class-weight-*`, `palw-seat-coverage-*`, `palw-shard-plan-*`, `evidence-qwen36-model-gate/` |
| plans and status ledgers that have been overtaken | `palw-road-to-mainnet-*`, `palw-two-class-plan-*`, `palw-practical-runtime-plan-*`, `palw-class-activation-gate-status.md`, `palw-rc-launch-blockers-*`, `palw-rc-t12-seat-deadlock-*`, `palw-qwen25-*`, `palw-private-prompts-design-*`, `palw-position-benefits-consumer-*`, `palw-economic-parameters-*`, `palw-economy-studio-drill-*`, `palw-stage0-shadow-drill-runbook.md` |
| completed migrations and old test fixtures | `hash64-migration-inventory.md`, `kaspa-pq-mldsa87-verification-runbook.md`, `mtp-epoch2-partition-policy.md`, `vps-regression-fixture.md`, `rc7-evidence.txt` |
| security audit response (2026-06-23) | `security/` |
| the testnet-11-era explorer patches and job exporter (2026-09) | `explorer/` |
| free-prompt wiring status notes, now landed | `palw-fp-on-registered-classes.md`, `palw-fp-wiring-atomicity.md` |

testnet-10 and testnet-11 documents were removed rather than archived; they remain in git history.
