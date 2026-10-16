# Audit

設計の前提は [../PRINCIPLES.md](../PRINCIPLES.md)、決定・実装・検証・有効化の現在地は [../STATUS.md](../STATUS.md)。この下の文書は**当時の監査・検証判定の記録**で、現行の既定値ではありません。現行の運用文書と食い違うときは、現行の運用文書(`--help` と chain RPC で確認)を使います。

各文書の移動元は [../history/MOVED.md](../history/MOVED.md)。

### PALW — 監査と検証判定(`palw/`)

- [ambient-pol-binary-audit-2026-08-15.md](palw/ambient-pol-binary-audit-2026-08-15.md) — Ambient PoL binary audit — llama.cpp-ambient-bin, 2026-08-15
- [palw-algo4-forgery-audit-2026-08-16.md](palw/palw-algo4-forgery-audit-2026-08-16.md) — PALW algo_id=4 偽造耐性監査 — 61推論・60 seed規模 (2026-08-16)
- [palw-audit-2026-09-18-6001.md](palw/palw-audit-2026-09-18-6001.md) — PALW DAA 6,000 / 6,001 pre-arming security audit — 2026-09-18
- [palw-base0-runtime-review-reconciliation-2026-08-26.md](palw/palw-base0-runtime-review-reconciliation-2026-08-26.md) — BASE-0 ランタイム設計レビューの照合 — 2026-08-26
- [palw-critical-audit-2026-08-19-ja.md](palw/palw-critical-audit-2026-08-19-ja.md) — PALW 致命的欠陥監査
- [palw-daa-clock-audit-2026-09-18.md](palw/palw-daa-clock-audit-2026-09-18.md) — The DAA clock under the 6,001 bundle — lane inventory, window arithmetic, verdict (2026-09-18)
- [palw-external-audit-2026-08-21.md](palw/palw-external-audit-2026-08-21.md) — External PALW audit — received 2026-08-21
- [palw-mainnet-audit-2026-08-28.md](palw/palw-mainnet-audit-2026-08-28.md) — PALW ConsensusV2 — mainnet audit (2026-08-28)
- [palw-mainnet-audit-2026-08-30.md](palw/palw-mainnet-audit-2026-08-30.md) — PALW mainnet audit, 2026-08-30 — the union build
- [palw-mainnet-audit-2026-09-05.md](palw/palw-mainnet-audit-2026-09-05.md) — PALW mainnet audit, 2026-09-05 — the carded-mainnet pass
- [palw-mainnet-audit-2026-09-06.md](palw/palw-mainnet-audit-2026-09-06.md) — PALW mainnet audit, 2026-09-06 — the two-tree pass
- [palw-mainnet-audit3-2026-08-29.md](palw/palw-mainnet-audit3-2026-08-29.md) — PALW — third-pass audit of the mainnet remediation, and the testnet-11 re-mint decision (2026-08-29)
- [palw-mainnet-readiness-audit-2026-08-22.md](palw/palw-mainnet-readiness-audit-2026-08-22.md) — PALW ConsensusV2 — mainnet readiness audit (2026-08-22)
- [palw-mainnet-reaudit-2026-08-29.md](palw/palw-mainnet-reaudit-2026-08-29.md) — PALW — mainnet re-audit of the 2026-08-28 remediation (2026-08-29)
- [palw-only-v4-audit-2026-08-17-ja.md](palw/palw-only-v4-audit-2026-08-17-ja.md) — PALW palw-only-v4 監査レポート — 2026-08-17
- [palw-rc-launch-blockers-2026-08-21.md](palw/palw-rc-launch-blockers-2026-08-21.md) — PALW-RC (testnet-12) — launch blockers, 2026-08-21
- [palw-release-6001-verdict-2026-09-18.md](palw/palw-release-6001-verdict-2026-09-18.md) — The 6,001 release: what was gated, what was measured, and the verdict

### EVM — 監査 scope(`evm/`)

- [audit-scope-evm-rpc.md](evm/audit-scope-evm-rpc.md) — Audit scope — MISAKA EVM lane + Ethereum JSON-RPC adapter + bridge + indexer

### 日付付きの監査の束(ディレクトリ)

- [2026-09-19-llm-mining/](2026-09-19-llm-mining/)
- [g14-review-2026-10-10/](g14-review-2026-10-10/)
- [g14-review-2026-10-13/](g14-review-2026-10-13/)
