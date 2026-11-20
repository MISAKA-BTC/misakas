# AN1改訂の変更点

## 方式の変更

前回whitepaperのFULL1/VF1から、PALW-AN1の実入力束縛とPanel＋Courtへ変更した。Groth16の必須化・setup・vk・proof cacheによるFinalを採用方式から外し、Panel完全再実行、認証付き局所反証、一人正直の条件へ置き換えた。

## RFCごとの変更

| 対象 | 修正・追加 |
| --- | --- |
| RFC-0056 §17 | C/nonce候補の事前freeze、AN1 Ticket、共通D一回、厳密target境界、TimingPlan、資金・予約、重複slot防止 |
| RFC-0057 §17 | Miner/Panel nonce分離、native期待Root、期待weight index、資料の実bytes再認証、証拠分類、全体再実行・反証優先 |
| RFC-0058 §16 | 別family/codec、二つのfreeze、C/slot/s/実仕事、全consumerの一回性、費用定理の限界、移行 |
| RFC-0059 | AN1の一体仕様。入力、実行、全trace、Court、Final、費用・容量・会計、有効化gateを追加 |
| ADR-0227 | 最新ユーザー指示に基づくPanel＋Court維持、設計理由、代案の判断と未完了条件 |
| RFC-v2の20冊 | 担当範囲のAN1草案参照を追加。既存の採択本文と過去snapshotを保持 |
| 入口・番号台帳 | 未使用を添付内で確認した0059/0227をDRAFTとして案内。過去next_localと採択mapを保持 |

## 白書の修正

18章の本文をAN1へ再構成した。Cとinput rootの非循環性、単射token表、実入力と通常promptの意味差、全local関係の帰納法、first-divergence、資料不足と偽証拠の区別、正直Panelの条件、原子Final、qの固定上限、c_minの未取得を明記した。

旧FULL1のbC≥KN/18NをAN1へ流用しない。正しいcache・簡約・外注をslashしない。別nonceの実入力が異なることから通常LLMと同額の費用を推論しない。

## 保全

添付の旧原則・AGENTS、RFC-0055、旧ADR-0223〜0226、過去maintenance/evidence/sourceのbytesを保持する。旧検査の結果を今回の文書へ再ラベルしない。完全な変更一覧・元bytesのhashは同梱`changes.json`と`source-baseline.json`に記録する。

新規実装は独立toy。既存runtimeコード・稼働ネットワーク設定・公開activationを変更していない。
