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

## 接続時の判定

- 初回 wide capture の元 claim には、step tree と logits trace を束縛した native execution root と、その実行の output root を設定する。現行 capture は Flat 入力を要求する。Merkle 入力の資料取得処理をこの capture の対応とみなさず、採用 profile の入力形式を実行側と一致させる必要がある。
- Program は正規 TIR v1 全体を単一チャンクとし、元 claim の graph IR root を既存の native hash で照合してから decode する。Artifact inventory の leaf は tensor・layer・row 範囲に対応し、入力 Merkle leaf は token tile に対応する。各 leaf を固定幅のバイト範囲と同一視せず、それぞれの座標と既存 opening を結ぶ codec が必要。
- 資料手続きは未確立。署名、URL、別 Panel の receipt、部分的な chunk、期限の経過から全 scope の検査機会を認定しない。各 case に認証できる供給根拠、本人の取得・検査・提出機会、残予算が必要。
- Final の入力は native state から導出する。全体承認、局所検査、notice 終了、検査機会、Q、依存、責任、資金、原期限を一か所で評価する。反証・失効・Abort を成功へ戻さない。
- Final の consumer は、仕事報酬、escrow、worker・service 支払、mint、安全 frontier と weight、settlement depth、Rule E、settled anchor、EXEC credit と予約、schedule、permit、依存の export と use。供給済みサービスの対価と返金は固有の条件を保つ。
- 検証 runner は通常の V1〜V4 と長時間の V5/V6 を分ける。対象 profile、ケース名と件数、実アーティファクト、機材、必要資源上限は候補固定時に記録する。現時点の V1〜V6 は未実施。

公開 T12 の Studio TFP1 は別途、現在の G14 ルート上で同じ claim ID の受理、対話、Final、ブロック生成を照合する。新 profile の契約や既存の T12 復旧試験だけで、この結果を記録しない。
