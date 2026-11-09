# PALW実行正当性 — 独立レビュー記録

レビュー対象は現行RFC16〜20、PRINCIPLES§13、追加採択0049／0217、公開裁定v0.3、および関連RFCの新版境界である。[固定基準](REVIEW-CRITERIA.md)を途中で弱めず、執筆に参加していないエージェントによる指摘を修正して再レビューする。

## Round 1 — core仕様: FAIL

独立reviewer Aは5冊全本文を読んで、次の設計上の隙間を指摘した。このFAILは実装や性能が未測定だからではなく、規範の解釈に攻撃経路が残ったためである。

| ID | 反例／不足 | 修正 |
| --- | --- | --- |
| B1 | 受理済み正当caseへ無効opening・誤署名packetを送ると、20の表を根拠にcaseを閉じられる | packet拒否とcase終了を分離。Q・手番・被害者の保護予算を維持。正規terminalや客観defaultだけがそのcaseを閉じる |
| B2 | notice窓内の資料要求へ窓後に応答し、配送履行でQを消すと、取得後に反証する権利がなくなる | 供給義務と検証caseを分離。InspectionPendingを保持し、同CaseId・scope・原時計・残予算で検査／反証を継続。残時間不足は成功不可 |
| B3 | 同じ要求を悪意者が先着し、正直な人の要求をdedup吸収してから自分のdefaultで共通caseを閉じる | 共有MaterialIdと申立人bond/principal付きCaseIdを分離。他人の終了で個別権利を消さない |
| B4 | 元Minerの認証済みrecordの不正codec・dtypeを、単なる申立人packet不正として捨てる | parseと独立なbounded raw認証、WellFormedの必須構成関係、元record違反の客観的反証を追加。開示不能・超大資料では架空の有罪を作らない |

対応箇所: RFC16 §4、17の承認境界、18 §2.1〜2.3／§4／§6、19 §4〜5.4、20 §5〜6と受入ケース。元の4指摘を削除してPASSへ書き換えてはいない。

## 統合レビュー — B5を追加修正

独立reviewer Bは、上位原則・採択記録・旧署名境界・EXECとの接続を確認した。

| ID | 指摘 | 修正 |
| --- | --- | --- |
| B5 | 同batchで既発行permitを先にconsumedへ移し、その後のsource反証を「既消費だから保持」で逃がせる解釈 | 20 §5.2／11Aと07／10の新版境界で、反証・Q hold・依存失効を全consumerより先に反映。既受理consumed保全はbatch以前のcanonical消費だけ |

§8の計算暗号proof不採用、§9のtier、§14のEXEC1 BPS、旧署名・歴史保存との整合を別に確認した。19のrelation schema表へWellFormedを加え、本文との用語をそろえた。

## Round 2 — core仕様: CONDITIONAL SPEC PASS

2026-11-09、独立reviewer A（`independent_review_round1`）が、固定したREVIEW-CRITERIAを変更せず、RFC16〜20の全本文と修正箇所の相互作用を再確認した。B5の接続先であるRFC07／10の新版境界も確認した。

**判定: 明示された前提の下でCONDITIONAL SPEC PASS。今回のレビュー範囲で未解決の設計BLOCKERなし。** 初回B1〜B4と追加B5の具体的反例は、修正後の規範で防がれていると判定された。

再確認した論理は、(1)元Minerの初回commitへの束縛、(2)WellFormed・Init・全Step・Endから実行正当性への対応、(3)偽の必須関係から有界な反証を構成できるprofileだけの受理、(4)肯定票から独立した一人の異議・反証、(5)資料不足から成功へ進む経路の禁止、(6)同batchの更新後stateを全成功・credit consumerが読むこと、である。

reviewer Aは抽象モデルの実行結果をこの判定の根拠にしていない。本文上の論理レビューと、別途実施した有限モデル検査を分離する。

## 最終統合レビュー: CONDITIONAL SPEC PASS

同日、独立reviewer B（`independent_integration_review`）が、PRINCIPLES §13、RFC-0049、ADR-0217、公開裁定policy v0.3、RFC16〜20および05／06／07／08／10／12／14の新版境界を再確認した。

**判定: 明示された前提の下でCONDITIONAL SPEC PASS。未解決の設計BLOCKERなし。** §8の計算暗号proof不採用、モデルtier、EXEC 1 BPS、旧署名・履歴、資料保持と復元、後日取消と依存先への効果について矛盾は見つからなかった。B1〜B5の修正を個別に再確認した。

最終統合レビューで残った非BLOCKERの編集指摘である「RFC19 §4のrelation schema表にもWellFormedを列挙する」は反映済みである。§5.2／5.4で既に義務化された内容と表記をそろえた変更であり、保証条件の変更ではない。

## 最終判定の範囲

**採択状態: ADOPTED_DESIGN_NOT_ACTIVATED。仕様の独立レビューは完了、実装・測定・有効化はNOT ESTABLISHED / NOT ACTIVATED。**

このPASSは、正しい検証者の能動的参加、必要資料へのアクセスまたは適格な取得不能手続き、資格・費用・個別に保護された有限予約、元の期限内の配送とcanonical包含、正しい認証・native検査器、実際に採用するprofileの局所完全性・健全性・有界な証拠構成可能性を前提とする。単に正直な一人が存在するだけで、全通信の検閲や無期限の資料秘匿を克服するという保証ではない。

Miner本人のGPU利用、各Panel本人の独立実行、実消費FLOPs、全員が共謀した場合の小さな全体肯定証明は保証に含めない。保証する計算正当性は、固定されたモデル・入力・数値規則に対する実行と出力・停止・仕事会計の整合性である。

実LLMの全kernel／schema／shape／長さとnative checkerの対応、正常Final・一人反証・不供給Abort、最低要件機材での最悪CPU／RAM／IOと包含、混合負荷、restart／reorg／IBD、同一binaryの試験は未実施である。RFC19 §5.3とRFC20 §11・11Bの有効化gateとして維持する。全profileを拒否するだけでは、利用可能なPALWの完成とは認めない。
