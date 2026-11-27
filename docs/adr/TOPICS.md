# ADR 分野別索引

[番号順](INDEX.md) · [RFC設計](../rfc/README.md) · [ADR判断理由](../adr/README.md)

## 原則・実行

- [ADR-0001 — 設計原則・暗号基盤・権限境界を選ぶ理由](0001-principles-and-authority.md)
- [ADR-0002 — 決定論的kernel・GPU・低RAMランタイムを選ぶ理由](0002-deterministic-kernels-and-runtime.md)
- [ADR-0003 — Job・tokenizer・生成契約を選ぶ理由](0003-jobs-and-tokenizer-contract.md)
- [ADR-0004 — 実行依存Ticket・仕事の一回性を選ぶ理由](0004-execution-bound-ticket-and-work-identity.md)
- [ADR-0005 — Qwen3の整数実行・37-vector Readoutを選ぶ理由](0005-qwen3-execution-and-readout.md)

## 乱数・モデル採用

- [ADR-0006 — 閾値乱数・公開規則・custodyを選ぶ理由](0006-threshold-randomness-and-custody.md)
- [ADR-0007 — BFTだけのモデル追加・採用を選ぶ理由](0007-bft-only-model-admission.md)

## 検証・Court・資格

- [ADR-0008 — G14の1席Panel・独立完全再実行を選ぶ理由](0008-one-panel-independent-reexecution.md)
- [ADR-0009 — 有界Court・反証優先・検証先行Finalを選ぶ理由](0009-bounded-court-and-verification-gated-final.md)
- [ADR-0010 — 公開資料・有限原資・供給者契約を選ぶ理由](0010-public-materials-and-funded-service.md)
- [ADR-0011 — G14と公開Courtに合意creditを結び付ける理由](0011-independent-gate-c-certification.md)
- [ADR-0012 — 用途別bond・予算予約・個別slashを選ぶ理由](0012-role-bonds-budgets-and-slashing.md)

## 仕事・合意・経済

- [ADR-0013 — モデル別仕事量・報酬・EXECの接続を選ぶ理由](0013-model-work-accounting.md)
- [ADR-0014 — EXEC・共通容量・難易度・合意時計を選ぶ理由](0014-exec-capacity-and-consensus-clock.md)
- [ADR-0015 — DAG・受賞ブロック・台帳決済を選ぶ理由](0015-dag-awards-and-settlement.md)
- [ADR-0016 — MOE3のPOSITION経済・永久流動性を選ぶ理由](0016-position-economics-and-permanent-liquidity.md)
- [ADR-0017 — モデル停止・解除・恒久廃止を選ぶ理由](0017-model-control-and-retirement.md)

## 保存・運用・移行

- [ADR-0018 — 有界保存・pruning・IBDを選ぶ理由](0018-bounded-storage-pruning-and-ibd.md)
- [ADR-0019 — クライアント・RPC・原因保持を選ぶ理由](0019-clients-and-cause-preserving-diagnostics.md)
- [ADR-0020 — devnet・公開有効化・文書管理を選ぶ理由](0020-devnet-activation-and-document-governance.md)

