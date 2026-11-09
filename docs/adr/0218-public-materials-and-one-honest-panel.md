# ADR-0218 — 公開資料とモデル未保有の一人検証

Status: **Accepted design / ADOPTED_DESIGN_NOT_ACTIVATED**
Date: 2026-11-09
Authority: この会話のユーザーによる、モデル未保有の一人の正直なPanel・公開資料・条件付き正常完了を含む提示方針に基づくdocs修正の明示指示。
RFC: [RFC-0050](../rfc/0050-public-materials-and-one-honest-panel.md)
Current owners: [RFC-v2-0016〜0020](../rfc-v2/README.md)

本書は統合後の追加採択記録であり、ADR-v2の51冊目を作らない。0218は利用できた資料内で未使用の番号を割り当てた。未取得branchの番号予約は保証しない。以前の0049／0217の反証完全性・単独異議・経済効果を維持して、新profileに次の条件を加える。

## 問題

モデルの事前保有を選出資格や検証開始の条件にすると、保有者だけが承認を支配できる。資料を持たない正直なPanelに算術反証を要求しても、秘匿された重みから証拠は作れない。また一人だけが正直という条件で正常claimも完了させるには、悪意多数の不署名による停止を避ける必要がある。

## 決定

[RFC-0050](../rfc/0050-public-materials-and-one-honest-panel.md)と[PRINCIPLES §13](../PRINCIPLES.md#model-free-adversarial-verification)の限定変更を採択する。

- 各claim、または固定後抽選の有限batch全体に、少なくとも一人の能動的な正直な選出者がいることを前提とする。Nodeが当人を識別できるとはしない。
- 重み・program・prompt・token化・数値規則・seed・依存状態・初回trace等の全必要資料を認証付きで公開供給する。事前保有を選出資格にせず、Minerの許可に依存しないprovider・cache・再配布・適格要求を認める。
- cold取得を受理計画へ含め、取得不能でも適格caseを開ける。供給履行と検査完了を分け、原時計とInspectionPendingを維持する。
- 新profileの最低完全検証承認数をlogicalに **k_min = 1** とする。全検査・異議の機会、Qの解消、必須局所検査、依存・資金・期限を満たして初めてFinalへ進める。多数の不署名だけを失敗理由にしない。
- 一人の有効反証は肯定票数不問で勝つ。正常claimへの最大合法な虚偽caseも、有界な応答・terminal・客観的defaultで元期限内に終了できる資源を条件付き完了の前提にする。
- モデルcache、固定後抽選の有限batch、初回commit済み計算区間で固定費を共有する。全体検証を部分担当へ置換せず、m削減や性能値をこの採択から導かない。

## 比較した案

| 案 | 判断 |
| --- | --- |
| モデル保有者だけからPanelを選ぶ | 未保有の正直者を排除し得るため不採用 |
| promptだけを公開し、重み・依存状態は提供しない | 全体再実行と反証が成立しないため新profileでは成功不可 |
| 一件の署名直後にFinalへ進める | 正直者の取得・検査・反証機会を失うため不採用 |
| 新profileでも41件の署名を必須にする | 一人以外が不署名なら正常claimの完了を保証できないため採らない |
| 一人正直だから任意の一人だけに再実行させる | 選ばれた担当が正直とは限らず、不採用 |
| 全資料をNodeへ配り可用性を救済する | モデル非保有・有界courtを破るため不採用 |
| 抽選上限だけを増やす | 全再実行・配信・係争の容量が別に必要なため、それだけで成立としない |

## 移行と保存

この意味変更は新しいclaim/profileだけへ適用する。旧75／41、旧4,096抽選位置／日、旧署名・wire・fence・既存runtimeを変更しない。新profileのm、各資源値、B、稼働policy、activationは実装・実測と別の確定記録を必要とする。最低承認数1をPanel人数1へ読み替えない。

[ADR-0217](0217-palw-execution-integrity-and-unilateral-refutation.md)と[公開裁定v0.3](../design/palw/consensus/public-adjudication-policy-v0.3.md)は先行採択記録として保存し、今回範囲について[公開裁定v0.4](../design/palw/consensus/public-adjudication-policy-v0.4.md)と現行16〜20を適用する。通常編集先20冊・統合ADR50冊と過去の検査証拠を維持する。

## 検証と残る条件

[今回の改訂](../maintenance/revisions/2026-11-09-public-panel/README.md)に独立レビュー、修正、検査結果を記録する。未検査をPASSに変換しない。モデルを事前保有しない唯一正直なPanelでの正常・反証・選択的秘匿・遅延・悪意不署名・虚偽case・最大負荷を同一実装で通すことが必要である。

保証は正直者の能動的な完全検証、認証とnative checkerの正しさ、必要資料または適格要求、費用・個別予約・期限内包含・実サービスを前提とする。正しいMinerの応答と最大虚偽caseの有限終結が正常完了の追加条件になる。全員共謀、永久検閲、無条件配送、本人の物理的計算を保証しない。

既存の報酬式不整合、beacon/bootstrapの接続、実LLM全kernelと資源測定は別のgateである。PRINCIPLES §8／§9／§14、既存経済値、延期対象を変更しない。**設計採択済みであり、実装・実測・稼働有効化の完了ではない。**

大きな資料の供給・取得後検査を有界なnative根拠で客観終結するprofile別手順は、具体化できていない場合 **PROFILE PROCEDURE NOT ESTABLISHED** として有効化不可である。最低承認数1や公開方針だけでは、この手順の成立を証明しない。実装・実測の未完と区別し、18 §2.4／20 §4.3のgateを維持する。
