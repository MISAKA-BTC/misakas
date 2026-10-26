# ADR-0192 — court の正規処理機会と共有訴追経路

- **Decision:** Proposed / Review Required (2026-10-20)。採択・統合・有効化を意味しない。
- **Scope:** [RFC-0026](../rfc/0026-court-policy-master-contract.md) §4・5、監査 F02・F03。
- **Source:** `7fe073ec2`。別 branch の scheduler・tag 245 はこの snapshot に未統合。
- **Supersession:** 採択前は既存の期限・署名・責任・wire を置換しない。採択後も旧 claim は旧 ruleset。

## 詳細仕様化の指示

2026-10-20、ユーザーは推奨 A/A の詳細仕様化を指示。[詳細契約案](../design/palw/integration/rfc26-service-shared-frontier-contract-2026-10-20.md)を追加した。数値・配送前提・wire・状態遷移の採択や武装承認とは区別する。

## 判断が必要な理由

チェーン上の応答不在だけでは、沈黙と個別配送妨害を区別できない。block の空き容量や進んだ DAA だけを「送信機会」とすると、同じ履歴の正直な署名者を処罰し得る。全 owner 枠の占有下で完成済み直接反証だけを受理しても、外部者が必要な開示に到達できることにはならない。fee や空き枠の予約だけでは両方を閉じない。

## F02 の選択肢

| 案 | 合意で記録するもの | 保証と限界 |
| --- | --- | --- |
| A: 予約サービス機会 + 明示的 inclusion 前提 | claim/争点/root/範囲/手番/責任鍵、必要 bytes/work、予約割当、予定 DAA と実供給、完了条件 | 各 accepting block の割当から機会を再計算。予約が提供され、かつ明示的な配送・収録前提の内側でのみ無応答 default。個別検閲をチェーンから識別できるとは主張しない |
| B: 無応答は非 credit の保留・復旧へ送る | 同じ義務と割当、未処理量、保持責任、復旧状態 | 計算不正の直接反証は裁定可能。無応答だけでは slash しない。永久沈黙時の担保保持と frontier 停滞が残り、無条件の有限返還は保証しない |

**提案:** A を仕様化する。ただし予約の供給を定義する受理規則と inclusion 前提はユーザー採択まで未決。正規機会なしは slash せず、hard deadline の内側で期限補正し Final を保留。hard deadline を越える時計 reset や自動無罪にしない。範囲外では未解決のまま担保・証拠を保持する。

A のレビューでは少なくとも、DAA ごとの複数 block / 無 block、通常 tx と court 予約の優先順位、署名検証前の無効入力予算、応答が入らなかった予約枠の供給認定、同一 block の最終応答と sweep の順序を固定する。ローカル回線・mempool・Panel の申告を入力にしない。供給ゼロでも絶対時計は進み、永久沈黙で時計を止めない。

## F03 の選択肢

| 案 | 経路 | 保証と未決 |
| --- | --- | --- |
| A: claim 共通の局所化 frontier | original acceptance で敵対的分岐を含む資源を予約。owner に依存しない争点/root/範囲/手番の要求を誰でも出し、指定 custodian の認証済み回答を共有する | owner の先着・上限 8 から独立。ただし許容分岐集合、重複統合、異なる合法 choice のスケジュール、最大 bytes/work と admission 検査が必要 |
| B: 有界な全開示を original acceptance で予約 | 外部者が必要な全公開資料を共有取得し、ローカルで局所化して直接反証 | owner 枠の争奪を除けるが全開示費用・保持・長 context の容量は未実証。上限引き上げや未知量のゼロ置換は不可 |

**提案:** A を tag 245 の共有開示を土台に仕様化する。claim ごとの最初の予約を共謀者が消費しても正直な争点が残ることを、争点 identity と許容する全分岐の容量から示す。「外部者 1 枠を残す」「reserve queue がある」だけで合格にしない。T05 は未完成証拠で入った外部者について、全 owner 占有・合法な最遅応答・埋め直し・同 root 重複を同時に試す。

## 現時点の実装境界

`palw_g14_gates_v1` に `OneOutsiderCapacityIncomplete` / `ServiceOpportunityIncomplete` の拒否を追加。通常 network では形式の正しい readiness record を渡しても未完経路を武装できない。既存の隔離 G14 test genesis は未完経路の試験を継続できる。preset・identity・wire・旧 claim の fold は変更しない。

`palw_court_policy_v1` は original accepting-chain DAA から 3,120 / 10,000、保持、依存 horizon、同じ bond の最大責任を checked arithmetic で導出する部品。state/fold 未接続であり、期限の原子的な状態更新や T06/T09 の完了とは表示しない。notice 等号・turn duration は選んでいない。

## 採択後の受入条件

T02–T05、T06–T09、T13–T15 を同じ release binary で実施。予約・effective terms・保留を reorg/restart/pruned IBD から同じ状態に戻す。t12 同一性試験を通す。測定値、derived_upper、未被覆を分離する。tag 230 の codec、応答 fee、turn duration、SC64 profile はこの ADR で決めない。

## 追補(2026-10-26、lane F03b2): F03 A の未決 2 点の暫定決定

**暫定。ユーザーが変更し得る。** 2026-10-22 の値決定委任(目安可、根拠と見直し条件を台帳に)により統括が決定。

1. **request ごとの admission 予算(目安):** bytes = 1 court packet(V1、34,000 B)、work = その操作 class の op_work(V11 `[5,13,1,11]`、request=Opening、body/IDs/root/round=DissectionMove、bottom=Close)。超過は状態変更なしで拒否(tag 178 入口)。M-C 実測で置換する。
2. **重なる合法 choice:** 同じ座標は従来どおり統合。異なる choice は応答期限の早い順、同順位は座標 id の昇順で決定論的に処理。受付時に `Q ≤ c·W`(Q=閉じた request 集合の大きさ、c=予約 packet/DAA、W=hard 窓)を要求し、各 request ≤ 1 packet なので k 番目の処理 DAA は `a + ⌈k/c⌉ − 1 < a + ⌈Q/c⌉ ≤ a + W = h`。共謀者の先取り・埋め直しは同じ集合の座標で、統合により job を増やさない。
3. 実装: `consensus/core/src/palw_court_shared_frontier_budget_v1.rs`。fold は処理 DAA をまだ記録しない(純関数と受付検査のみ)。
