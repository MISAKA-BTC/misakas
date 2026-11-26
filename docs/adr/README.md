# ADR — 設計を選んだ理由

[総合入口](../README.md) · [番号順](INDEX.md) · [分野別](TOPICS.md) · [旧参照対応](MAPPING.md) · [RFC](../rfc/README.md)

ADRは背景、決定、判断理由、比較した選択肢、帰結を記す。実装上の正本は同じ番号のRFCであり、数式・データ形式・状態遷移・検査条件をADRへ重複記載しない。

20本に統合し、旧235本の本文は削除する。旧本文のarchive、履歴コピー、v1/v2別体系は作らない。対応表は過去の参照の読み替えに限る。提案は提案のまま残し、文書整理によって採択・有効化されたものと扱わない。

| 判断理由 | 対応仕様 |
| --- | --- |
| [ADR-0001 設計原則・暗号基盤・権限境界](0001-principles-and-authority.md) | [RFC-0001](../rfc/0001-principles-and-governance.md) |
| [ADR-0002 決定論的kernel・GPU・低RAMランタイム](0002-deterministic-kernels-and-runtime.md) | [RFC-0002](../rfc/0002-tensor-ir-kernels-and-runtime.md) |
| [ADR-0003 Job・tokenizer・生成契約](0003-jobs-and-tokenizer-contract.md) | [RFC-0003](../rfc/0003-jobs-tokenizers-and-generation.md) |
| [ADR-0004 実行依存Ticket・仕事の一回性](0004-execution-bound-ticket-and-work-identity.md) | [RFC-0004](../rfc/0004-fow1-execution-and-ticket.md) |
| [ADR-0005 Qwen3の整数実行・37-vector Readout](0005-qwen3-execution-and-readout.md) | [RFC-0005](../rfc/0005-qf1-qwen3-execution-profile.md) |
| [ADR-0006 閾値乱数・公開規則・custody](0006-threshold-randomness-and-custody.md) | [RFC-0006](../rfc/0006-tb1-randomness-and-release.md) |
| [ADR-0007 BFTだけのモデル追加・採用](0007-bft-only-model-admission.md) | [RFC-0007](../rfc/0007-bft-model-admission.md) |
| [ADR-0008 G14の1席Panel・独立完全再実行](0008-one-panel-independent-reexecution.md) | [RFC-0008](../rfc/0008-g14-panel-and-verification.md) |
| [ADR-0009 有界Court・反証優先・検証先行Final](0009-bounded-court-and-verification-gated-final.md) | [RFC-0009](../rfc/0009-court-and-claim-final.md) |
| [ADR-0010 公開資料・有限原資・供給者契約](0010-public-materials-and-funded-service.md) | [RFC-0010](../rfc/0010-materials-and-funded-delivery.md) |
| [ADR-0011 独立Gate C・合意credit認証](0011-independent-gate-c-certification.md) | [RFC-0011](../rfc/0011-gate-c-and-consensus-credit.md) |
| [ADR-0012 用途別bond・予算予約・個別slash](0012-role-bonds-budgets-and-slashing.md) | [RFC-0012](../rfc/0012-bonds-budgets-and-slashing.md) |
| [ADR-0013 モデル別仕事量・報酬・EXECの接続](0013-model-work-accounting.md) | [RFC-0013](../rfc/0013-model-work-accounting.md) |
| [ADR-0014 EXEC・共通容量・難易度・合意時計](0014-exec-capacity-and-consensus-clock.md) | [RFC-0014](../rfc/0014-exec-schedule-and-capacity.md) |
| [ADR-0015 DAG・受賞ブロック・台帳決済](0015-dag-awards-and-settlement.md) | [RFC-0015](../rfc/0015-dag-blocks-and-settlement.md) |
| [ADR-0016 MOE3のPOSITION経済・永久流動性](0016-position-economics-and-permanent-liquidity.md) | [RFC-0016](../rfc/0016-position-market-and-rewards.md) |
| [ADR-0017 モデル停止・解除・恒久廃止](0017-model-control-and-retirement.md) | [RFC-0017](../rfc/0017-model-stop-and-retirement.md) |
| [ADR-0018 有界保存・pruning・IBD](0018-bounded-storage-pruning-and-ibd.md) | [RFC-0018](../rfc/0018-storage-pruning-and-ibd.md) |
| [ADR-0019 クライアント・RPC・原因保持](0019-clients-and-cause-preserving-diagnostics.md) | [RFC-0019](../rfc/0019-clients-rpc-and-diagnostics.md) |
| [ADR-0020 devnet・公開有効化・文書管理](0020-devnet-activation-and-document-governance.md) | [RFC-0020](../rfc/0020-devnet-release-and-migration.md) |
