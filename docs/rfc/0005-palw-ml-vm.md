# RFC-0005: PALW VM（不採用）

**Status:** Withdrawn — 2026-10-06、[ADR-0172](../adr/0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md) により実装方針を撤回。

PALW-BVM・PALW-GVM は実装しない。ISA、syscall、guest メモリ、gas、VM court、toolchain を合意規則として維持すると、モデル対応に必要な実装・監査・履歴再生の負担が増えるため。
モデル拡張には [versioned Kernel と宣言的 plan](../design/palw/versioned-kernels.md) を使い、未対応の意味論は審査済みの Kernel 更新で追加する。
