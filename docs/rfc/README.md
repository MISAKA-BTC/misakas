# MISAKA RFC — 実装設計

現行の設計仕様は **0001〜0020の20本**です。旧71本文・旧本文の複製・RFC-v1/RFC-v2の別体系はこの配布物に含めません。

各RFCは数式・形式・状態遷移・制約・受入条件を定めます。背景・選択理由・比較した案は[同じ番号のADR](../adr/README.md)へ置きます。

モデル追加は **BFT YES/NOのみ**です。kernel解析・lowering・互換検査は実行準備であり、承認を代替しません。個別claimのG14 Panelと公開Courtは維持します。

| 現行番号 | 設計の範囲 |
| --- | --- |
| RFC-0001 | [設計原則・責務・適用範囲](0001-principles-and-governance.md) |
| RFC-0002 | [PALW-TIR・kernel・推論ランタイム](0002-tensor-ir-kernels-and-runtime.md) |
| RFC-0003 | [Job・tokenizer・決定論的生成](0003-jobs-tokenizers-and-generation.md) |
| RFC-0004 | [FOW1実行・Readout・Ticket・仕事の一回性](0004-fow1-execution-and-ticket.md) |
| RFC-0005 | [QF1：Qwen3の具体的実行・Readout](0005-qf1-qwen3-execution-profile.md) |
| RFC-0006 | [TB1：用途別乱数・公開・復元](0006-tb1-randomness-and-release.md) |
| RFC-0007 | [BFTによるモデル追加・採用](0007-bft-model-admission.md) |
| RFC-0008 | [G14 Panel・完全再実行・肯定承認](0008-g14-panel-and-verification.md) |
| RFC-0009 | [Court・単独反証・claim Final](0009-court-and-claim-final.md) |
| RFC-0010 | [モデル資料・共有供給・有限資金契約](0010-materials-and-funded-delivery.md) |
| RFC-0011 | [G14・公開Courtによる合意credit](0011-gate-c-and-consensus-credit.md) |
| RFC-0012 | [用途別bond・予算・Court slash](0012-bonds-budgets-and-slashing.md) |
| RFC-0013 | [MWA1：モデル別仕事量・報酬原資](0013-model-work-accounting.md) |
| RFC-0014 | [EXEC credit・schedule・共通容量・難易度](0014-exec-schedule-and-capacity.md) |
| RFC-0015 | [DAG・PALWブロック・台帳決済](0015-dag-blocks-and-settlement.md) |
| RFC-0016 | [MOE3：POSITION市場・報酬・永久流動性](0016-position-market-and-rewards.md) |
| RFC-0017 | [モデル停止・解除・BMR1恒久廃止](0017-model-stop-and-retirement.md) |
| RFC-0018 | [状態保存・pruning・IBD](0018-storage-pruning-and-ibd.md) |
| RFC-0019 | [Studio・Miner・RPC・原因診断](0019-clients-rpc-and-diagnostics.md) |
| RFC-0020 | [ローカルdevnet・公開条件・移行](0020-devnet-release-and-migration.md) |

設計採択、実装完了、試験成功、公開有効化は別です。MWA1（RFC-0013）と新しい認証state map（RFC-0018の提案節）は提案のままです。

[設計原則](../PRINCIPLES.md) · [旧参照の対応表](MAPPING.md) · [文書検査](../maintenance/README.md)
