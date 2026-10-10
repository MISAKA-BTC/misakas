# MISAKA PALW float-worker OTA — 不採用理由

Status: **Withdrawn / 不採用。**

旧 float worker / llama.cpp runtime plugin 専用の fleet OTA 導入案は継続しない。対象の実行 family 自体が撤回され、専用 updater・runtime manifest・rollout 基盤を追加する必要がなくなったため。対象の撤回は [ADR-0053](adr/0053-palw-one-execution-family.md) に従う。artifact の導入成功を consensus activation の承認として扱わない原則は引き続き有効である。
