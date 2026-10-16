# Design

設計書・設計記録・計画。**採用された決定は [../adr/](../adr/README.md)、提案は [../rfc/](../rfc/README.md)、現在地は [../STATUS.md](../STATUS.md)。** ここにあるものは、決定が実装・検証・有効化されたことを意味しません。移動元は [../history/MOVED.md](../history/MOVED.md)。

- [palw/](palw/) — PALW の lane・matrix・ユーザー決定の記録・旧設計(`palw/` の既存ファイルは再編で動かしていない。root から移した旧 PALW 設計・計画 19 本を追加した)
- [evm/](evm/) — EVM lane・state backend・wallet 資産管理の設計(撤回済みの三 lane・Token Program を含む)
- [consensus/](consensus/) — PQ(ML-DSA-87)移行と Hash64 の設計・棚卸し

### evm

- [misaka-base-3lane-execution-design-v0.1.md](evm/misaka-base-3lane-execution-design-v0.1.md) — MISAKA Base 三つの EVM lane — 不採用理由
- [misaka-compute-token-program-design-v0.1.md](evm/misaka-compute-token-program-design-v0.1.md) — MISAKA Compute Token Program v0.1 — 不採用理由
- [misaka-evm-design-v0.4.md](evm/misaka-evm-design-v0.4.md) — MISAKA/Kaspa L1 Selected-Parent EVM 設計書 v0.4
- [misaka-evm-optimization-design-v0.1.md](evm/misaka-evm-optimization-design-v0.1.md) — MISAKA EVM レーン最適化設計書 v0.1
- [misaka-evm-state-backend-design-v0.1.md](evm/misaka-evm-state-backend-design-v0.1.md) — MISAKA EVM State Backend — Design v0.1 (audit C-01 remediation)
- [misaka-pq-deposit-strict-design-v0.1.md](evm/misaka-pq-deposit-strict-design-v0.1.md) — MISAKA Strict PQ-Rooted EVM Deposit — Design v0.1 (audit QR-C01 remediation)

### consensus

- [hash64-migration-inventory.md](consensus/hash64-migration-inventory.md) — PR-9.5 — Hash → Hash64 Consensus Identity Cascade — Migration Inventory
- [kaspa-pq-design-mldsa87.md](consensus/kaspa-pq-design-mldsa87.md) — kaspa-pq 量子耐性完了設計書
