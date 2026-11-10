# PublicOneHonest 実装台帳

[実装方針書](public-one-honest-vertical-implementation-plan-2026-11-10.md) の I0 作業記録。出荷設定と公開ネットワークでの有効化はこの台帳の対象外。以下の既存コードは接続先であり、新 profile の検証結果は未取得。

## 経路と接続先

| 経路 | 既存の接続先 | 新 profile の残件 |
| --- | --- | --- |
| 正常 claim | [Miner 推論](../../../../misaka/palw/remote/src/positive_miner_v2_inference.rs)、[一回の実行からの capture](../../../../misaka/palw/tir/exec/src/node/positive_wide_material_v2/capture.rs)、[Result 固定後の選出と認証](../../../../consensus/core/src/palw_state_v2/positive_assignment_certification.rs) | 同じ実行の初回 trace と manifest を Miner が署名。公開 RPC、native original、選出 bond による全体承認、局所検査、Final を接続 |
| 資料不足 | [Panel 取得キャッシュ](../../../../misaka/palw/remote/src/positive_panel_sources.rs)、[共有資料 intake](../../../../consensus/core/src/palw_court_shared_material_intake_v1.rs) | manifest 入口から全 scope の取得と検査機会までの profile 手続き。各申立人の Q、供給・防御応答、検査・提出枠を原時計で保存 |
| 一人の反証 | [TIR court](../../../../consensus/core/src/palw_tir_court_v1.rs)、[公開裁定 fold](../../../../consensus/core/src/palw_tir_public_fold_v1.rs) | Miner の初回 root に対する関係別 checker、Q 継続、元 claim の失効、依存取消、帰責別の費用・返還・slash |
| 復元 | [virtual processor](../../../../consensus/src/pipeline/virtual_processor/processor.rs)、[PALW state store](../../../../consensus/src/model/stores/palw_state_v2.rs) | original、draw、Q、予約、認証、Final、credit を同じ delta で保存。restart、reorg、pruning、IBD の経路から復元 |
| credit と決済 | [native settlement](../../../../consensus/src/pipeline/virtual_processor/native_settlement.rs)、[EXEC fold](../../../../consensus/core/src/palw_exec_v2_fold.rs)、[round producer](../../../../kaspad/src/palw_round_producer.rs) | 同一 canonical 単位で Q・反証・依存失効を先に評価し、Final 後に一回だけ credit を発行。予約、ScheduleId、permit 消費、取消を接続 |

[共通契約](../../../../misaka/palw/positive-contract/src/public_one_honest.rs) は、元 claim、全体承認、manifest 自体への要求、共有 MaterialId と個別 CaseId、原受理からの期限、抽選前の救済容量の算術を定義済み。native の受理・認証・予約・決済は未接続。既存の署名ドメイン、wire 値、Gate C、二階層の意味は各版で維持する。

## 全実行関係の対応を確定するための台帳

| 関係 | 初回記録から固定する対象 | 接続と測定が必要な項目 |
| --- | --- | --- |
| WellFormed | raw 記録、schema、長さ、型、座標、各 root | 外側 packet の拒否と Miner が署名した raw の違反を分離。最大 parse、bytes、hash |
| Init | program、artifact、tokenizer、prompt と token 列、seed、初期 KV と依存 | 宣言の同一性、入力・状態の正規化、初期 opening。最大 I/O、heap |
| Step | read-from、演算・index・shape、区間還元、state 更新と非更新部分、RNG | 各 kernel の数値意味、初回 trace の区間境界、局所 witness と checker。最大算術、heap、bytes、hash、I/O |
| End | token 選択、停止、出力、長さ、仕事会計 | 最終 state と公開結果の対応、停止規則、会計導出。最大算術、parse、I/O |

対象 profile の実アーティファクト、program、tokenizer、数値規則の digest と、各行の実際の kernel・witness・上限が揃うまで coverage は未確定。全体再実行は Panel 側で行い、Node は個別関係の有界な検査を担当する。

## 資料 scope と認証経路

| scope | 現在コードで使える認証 | profile を接続するための残件 |
| --- | --- | --- |
| Artifact | 既存の inventory root と tensor・layer・row の opening | 可変長の leaf 座標を manifest の要求・予算に結び付ける |
| Program | 公開単一チャンクを元 claim の graph IR root と照合して正規 decode | native の供給義務と取得予算へ接続する |
| Prompt | 公開 UTF-8 原文を認証し、元 claim の tokenizer で再トークン化する | profile ごとの前処理と局所検査を固定する |
| InputTokens | 再生成したトークン列を Flat または Merkle の native commitment と照合する。wide 再実行は Flat に対応 | Merkle の wide 再実行と native の取得予算を接続する |
| Tokenizer | 公開 tokenizer JSON を native の raw commitment と照合してから parse する | 局所検査と native の供給義務を接続する |
| ExecutionRules | ジョブと元 claim の CU ruleset ID を照合 | 乱数・選択・停止・算術規則を実行 profile の資料として固定する |
| InitialState | 元 claim と rooted 入力の digest を照合 | 初期状態と先行実行からの読み込みの正規資料を取得・検査する |
| Dependencies | 元 claim と rooted 入力の digest を照合 | 空集合を含む正規形式、依存 ID の公開・失効経路を接続する |
| Trace | Miner の確定処理は初回 capture のタイルと opening を公開ストアに書く。Panel は公開索引と全タイルを有界取得し、元 claim と独立再実行に照合する。葉数と wide logits の裁定結果は元 claim ID と証拠 ID の組で参照できる | Miner と Panel の daemon への接続、全関係の局所化、裁定結果から native 状態更新への接続 |
| Output | 公開の生成 token 列を native output root と照合し、wide 再実行と比較する | 停止・生成長・会計を含む End 検査を接続する |

I1 のローカル統合テストでは、Miner が wide TIR を一度実行して署名・公開し、Panel は signed original を入口に HTTP で manifest と artifact の行を取得し、登録済み inventory root を検証して別の実行用テープで同じ original を再実行する。artifact の行や公開 trace のタイルが欠けると資料不足で停止し、改変された artifact も拒否する。全ノードを commit するステートレス wide TIR の検査計画について、同じ original と抽選済み bond に束縛した一括承認を再実行後に署名する。統合テストではその承認を native の carrier 検証に渡す。native fold は予約 ID 付き original の受理記録を包含点と照合して保存し、抽選点で draw を保存する。検証済み承認 carrier は包含点と保存済み draw が一致する場合に一 bond 一行で記録する。これらの差分適用、巻き戻し、スナップショット復元をローカルで検証済み。daemon での受付・予約の取得、抽選、必須局所検査、Final、決済は未接続。

固定長 manifest の取得と、資料供給義務に結び付いた署名済み応答の認証は別経路。前者は元 claim と資料の同一性、後者は当該 case への応答の帰属を確認する。どちらも単独では検査機会を成立させない。

## 接続時の判定

- 初回 wide capture の元 claim には、step tree と logits trace を束縛した native execution root と、その実行の output root を設定する。現行 capture が要求する Flat 入力は、manifest の単一チャンクから native flat hash で照合できる。Merkle 入力には既存の token tile opening を使い、実行側の対応は別に確認する。
- Program は正規 TIR v1 全体を単一チャンクとし、元 claim の graph IR root を既存の native hash で照合してから decode する。Artifact inventory の leaf は tensor・layer・row 範囲に対応し、入力 Merkle leaf は token tile に対応する。各 leaf を固定幅のバイト範囲と同一視せず、それぞれの座標と既存 opening を結ぶ codec が必要。
- 資料手続きは未確立。署名、URL、別 Panel の receipt、部分的な chunk、期限の経過から全 scope の検査機会を認定しない。各 case に認証できる供給根拠、本人の取得・検査・提出機会、残予算が必要。
- Final の入力は native state から導出する。全体承認、局所検査、notice 終了、検査機会、Q、依存、責任、資金、原期限を一か所で評価する。反証・失効・Abort を成功へ戻さない。
- Final の consumer は、仕事報酬、escrow、worker・service 支払、mint、安全 frontier と weight、settlement depth、Rule E、settled anchor、EXEC credit と予約、schedule、permit、依存の export と use。供給済みサービスの対価と返金は固有の条件を保つ。
- 検証 runner は通常の V1〜V4 と長時間の V5/V6 を分ける。対象 profile、ケース名と件数、実アーティファクト、機材、必要資源上限は候補固定時に記録する。現時点の V1〜V6 は未実施。

公開 T12 の Studio TFP1 は別途、現在の G14 ルート上で同じ claim ID の受理、対話、Final、ブロック生成を照合する。新 profile の契約や既存の T12 復旧試験だけで、この結果を記録しない。

2026-11-10 の公開 T12 では、同期済み seat 7 の受付見積もりが DAA 20,850 で対象の Studio claim をクラス未登録として拒否した。復旧後の seat 5 の作業ブロックは別の claim を含む。Studio の登録見積もりは取得済みで、登録の署名、受付、対話、Final、ブロック生成の記録は未取得。

同日 DAA 20,860 の seat 5 でも、同じ Studio データで登録事前検査は通り、claim の見積もりはクラス未登録として拒否された。新 profile の original 受け入れ記録は合意状態の行、差分適用・巻き戻し、スナップショット復元に接続済み。公開 RPC で受理された署名付き original から、同じ状態遷移で供給・保持を予約して記録を書く処理は未接続。

隔離 Linux ノードの旧ビルド `fa449130a` は同日 DAA 5,057 で、公開 Explorer の DAA 20,868 と pruning point は一致したが同期未完了。IBD は `unsupported native cold tip capacity` を繰り返し、一部の再試行では `IbdChainBlockLocator`／`BlockBody` の待受に別の応答が到着した。公開側は同期済み。新ビルド `244b69d93` で確認する対象は、この隔離環境での IBD 完了と公開側の sink への合流であり、Studio claim の実証とは分けて記録する。
